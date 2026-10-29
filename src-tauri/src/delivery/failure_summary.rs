//! What a failed command's output says went wrong, for the job's error and
//! the rounds that quote it. The last line of stderr is often a Node
//! warning while the failure is in stdout: both streams are read, the
//! failure reports of the usual test runners, compilers and linters are
//! picked out, and known noise is left aside.

const MAX_LINES: usize = 12;
const MAX_CHARS: usize = 1_500;
/// When no tool's report is recognised: the last lines that are not noise.
const FALLBACK_LINES: usize = 3;

/// The failure's lines from the command's `stdout` and `stderr`, empty when
/// it printed nothing but noise.
pub(super) fn summary(stdout: &str, stderr: &str) -> String {
    let lines: Vec<&str> = stdout
        .lines()
        .chain(stderr.lines())
        .map(str::trim)
        .filter(|line| !line.is_empty() && !noise(line))
        .collect();
    let mut picked = mocha(&lines).unwrap_or_else(|| marked(&lines));
    if picked.is_empty() {
        picked = lines[lines.len().saturating_sub(FALLBACK_LINES)..].to_vec();
    }
    picked.dedup();
    picked.truncate(MAX_LINES);
    let text = picked.join("\n");
    match text.char_indices().nth(MAX_CHARS) {
        Some((at, _)) => format!("{}…", &text[..at]),
        None => text,
    }
}

/// Lines that never say what failed: Node's and npm's chatter.
fn noise(line: &str) -> bool {
    line.starts_with("npm notice")
        || [
            "--trace-warnings",
            "MODULE_TYPELESS_PACKAGE_JSON",
            // The rest of that warning.
            "Reparsing as ES module",
            "To eliminate this warning",
        ]
        .iter()
        .any(|text| line.contains(text))
}

/// Mocha's `N failing` and the numbered failures after it, without their
/// stack frames.
fn mocha<'a>(lines: &[&'a str]) -> Option<Vec<&'a str>> {
    let at = lines.iter().rposition(|line| {
        line.strip_suffix(" failing")
            .is_some_and(|count| !count.is_empty() && count.bytes().all(|b| b.is_ascii_digit()))
    })?;
    Some(
        lines[at..]
            .iter()
            .filter(|line| !line.starts_with("at "))
            .copied()
            .collect(),
    )
}

/// The lines of `node --test`, cargo, tsc, eslint and npm that say what
/// failed.
fn marked<'a>(lines: &[&'a str]) -> Vec<&'a str> {
    let mut picked = Vec::new();
    let mut panic_message = false;
    for line in lines {
        // A panic's message is on the line after it.
        if std::mem::take(&mut panic_message) && !line.starts_with("note: ") {
            picked.push(*line);
            continue;
        }
        let eslint = {
            let mut words = line.split_whitespace();
            words.next().is_some_and(|at| {
                at.split_once(':').is_some_and(|(row, column)| {
                    [row, column]
                        .iter()
                        .all(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
                })
            }) && words.next() == Some("error")
        };
        let signal = eslint
            || (line.starts_with('✖') && line.contains("problem"))
            || line.starts_with("not ok ")
            || (line.starts_with("# fail ") && *line != "# fail 0")
            || line.starts_with("test result: FAILED")
            || line.contains("error TS")
            || line.starts_with("npm ERR!")
            || line.starts_with("npm error")
            || line.starts_with("error[")
            || line.starts_with("error:");
        if line.contains(" panicked at ") {
            panic_message = true;
            picked.push(*line);
        } else if signal {
            picked.push(*line);
        }
    }
    picked
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Real outputs of failed checks: stdout, then stderr.
    #[test]
    fn the_failure_is_found_in_either_stream_past_the_noise() {
        let mocha_stderr = include_str!("fixtures/mocha-stderr.txt");
        let cases = [
            (
                include_str!("fixtures/mocha-database-dropped.txt"),
                mocha_stderr,
                "database is in the process of being dropped",
            ),
            (
                include_str!("fixtures/mocha-hook-timeout.txt"),
                mocha_stderr,
                "Timeout of 60000ms exceeded",
            ),
            (
                include_str!("fixtures/mocha-fast-check.txt"),
                mocha_stderr,
                "Counterexample: [{[\"__proto__\"]:{}}]",
            ),
            (
                include_str!("fixtures/cargo-test-failed.txt"),
                include_str!("fixtures/cargo-test-failed-stderr.txt"),
                "assertion failed: validate_abi_image_id",
            ),
            (
                "",
                include_str!("fixtures/tsc-errors-stderr.txt"),
                "error TS2739",
            ),
        ];
        for (stdout, stderr, expected) in cases {
            let found = summary(stdout, stderr);
            assert!(found.contains(expected), "{expected} not in:\n{found}");
            assert!(!found.contains("--trace-warnings"), "{found}");
            assert!(
                found.lines().all(|line| !line.starts_with("at ")),
                "no stack frames:\n{found}"
            );
            assert!(found.lines().count() <= MAX_LINES, "{found}");
        }
        let mocha = summary(
            include_str!("fixtures/mocha-database-dropped.txt"),
            mocha_stderr,
        );
        assert!(
            mocha.starts_with("1 failing\n1) Auth tenant context"),
            "{mocha}"
        );
        // cargo: each panic with its message, and which tests failed.
        let cargo = summary(
            include_str!("fixtures/cargo-test-failed.txt"),
            include_str!("fixtures/cargo-test-failed-stderr.txt"),
        );
        assert!(
            cargo.contains("called `Result::unwrap()` on an `Err` value"),
            "{cargo}"
        );
        assert!(
            cargo.contains("test result: FAILED. 15 passed; 3 failed"),
            "{cargo}"
        );
    }

    #[test]
    fn other_tools_reports_and_the_fallback() {
        let cases = [
            (
                "TAP version 13\nok 1 - adds\nnot ok 2 - subtracts\n  ---\n# pass 1\n# fail 1\n",
                "not ok 2 - subtracts\n# fail 1",
            ),
            (
                "/repo/src/a.ts\n  12:5  error  'x' is assigned a value but never used  no-unused-vars\n  14:1  warning  Unexpected console statement  no-console\n\n✖ 2 problems (1 error, 1 warning)\n",
                "12:5  error  'x' is assigned a value but never used  no-unused-vars\n✖ 2 problems (1 error, 1 warning)",
            ),
            (
                "npm notice run build\nnpm ERR! code ELIFECYCLE\nnpm ERR! errno 2\n",
                "npm ERR! code ELIFECYCLE\nnpm ERR! errno 2",
            ),
            // Nothing recognised: the last lines that are not noise.
            (
                "building\nstep 1\nstep 2\nboom: disk full\n(Use `node --trace-warnings ...` to show where the warning was created)\n",
                "step 1\nstep 2\nboom: disk full",
            ),
        ];
        for (output, expected) in cases {
            assert_eq!(summary(output, ""), expected);
        }
    }
}
