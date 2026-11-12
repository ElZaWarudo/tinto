//! Tests the user marked as failing only because of the environment (a
//! Windows run without /proc, a flaky gRPC test): a check whose every
//! failure is one of them does not send the work back, while any other
//! failure still does.

use super::model::DeliveryKnownFailure;

/// The known failures `output` of check `check` shows, when they are all it
/// failed on: its failure count (mocha's "N failing", jest's or vitest's
/// "N failed", cargo's "test result: FAILED. … N failed", summed) is known
/// and each failure is one of them, named where the output shows failures
/// (a line marked ✕, ×, ✗, ● or FAIL, or the title of each numbered
/// failure after mocha's "N failing", its suites and title joined) as a
/// whole name. `›` and `>` between suites and title count as spaces, so a
/// test can be named by its title or by its suites and title. Each failure
/// counts once, for the longest known name on it, so a shorter name never
/// covers a longer test nor counts twice. `None` when any failure is not
/// known, or the count is not found.
pub fn only_known<'a>(
    check: &str,
    output: &str,
    known: &'a [DeliveryKnownFailure],
) -> Option<Vec<&'a DeliveryKnownFailure>> {
    let count = failure_count(output)?;
    let failing = failure_lines(output);
    let candidates: Vec<(&DeliveryKnownFailure, String)> = known
        .iter()
        .filter(|failure| failure.check.trim() == check.trim())
        .map(|failure| (failure, plain(&failure.test)))
        .filter(|(_, test)| !test.is_empty())
        .collect();
    let mut matched: Vec<&DeliveryKnownFailure> = Vec::new();
    let mut lines = 0;
    for line in failing {
        let line = plain(&line);
        let longest = candidates
            .iter()
            .filter(|(_, test)| names(&line, test))
            .max_by_key(|(_, test)| test.len())
            .map(|(failure, _)| failure);
        if let Some(failure) = longest {
            lines += 1;
            if !matched
                .iter()
                .any(|seen| seen.test.trim() == failure.test.trim())
            {
                matched.push(failure);
            }
        }
    }
    (count > 0 && lines >= count).then_some(matched)
}

/// `text` with `›` and `>` as spaces and its spaces collapsed.
fn plain(text: &str) -> String {
    text.replace(['›', '>'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Whether `line` names `test` as a whole (both `plain`): what comes before
/// it is the line's start, a space or a separator (`:`, `)`), and what
/// follows is the end, `:`, or a space before a duration, `...` or `[`.
fn names(line: &str, test: &str) -> bool {
    line.match_indices(test).any(|(at, _)| {
        let before = line[..at].chars().last();
        let after = &line[at + test.len()..];
        let starts = before.is_none_or(|c| c.is_whitespace() || matches!(c, ':' | ')'));
        let ends = after.trim().is_empty()
            || after.starts_with(':')
            || [" (", " ...", " ["]
                .iter()
                .any(|tail| after.starts_with(tail));
        starts && ends
    })
}

/// The failures the output counts, over every summary line it has; `None`
/// when it has none.
fn failure_count(output: &str) -> Option<usize> {
    let mut total = None;
    for line in output.lines() {
        let trimmed = line.trim_start();
        // Files and suites, not tests.
        if trimmed.starts_with("Test Files") || trimmed.starts_with("Test Suites") {
            continue;
        }
        let words: Vec<&str> = trimmed
            .split(|c: char| c.is_whitespace() || c == ';' || c == ',')
            .filter(|word| !word.is_empty())
            .collect();
        for pair in words.windows(2) {
            if matches!(pair[1], "failing" | "failed") {
                if let Ok(count) = pair[0].parse::<usize>() {
                    *total.get_or_insert(0) += count;
                }
            }
        }
    }
    total
}

/// Lines that show a failing test. After mocha's "N failing", each failure
/// is numbered ("1) suite"), with its nested suites and its title (ending
/// in `:`) on the lines below: they make one line, the error under it none.
fn failure_lines(output: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut after_failing = false;
    // The numbered failure whose title is still being read.
    let mut title: Option<Vec<&str>> = None;
    for line in output.lines() {
        let trimmed = line.trim();
        if after_failing {
            let numbered = trimmed
                .split_once(") ")
                .filter(|(number, _)| number.parse::<usize>().is_ok());
            if let Some((_, first)) = numbered {
                title = Some(vec![first]);
            } else if let Some(parts) = title.as_mut() {
                parts.push(trimmed);
            }
            if trimmed.ends_with(':') {
                if let Some(parts) = title.take() {
                    lines.push(parts.join(" "));
                }
            }
            continue;
        }
        if ["✕", "×", "✗", "●", "FAIL"]
            .iter()
            .any(|mark| trimmed.contains(mark))
        {
            lines.push(line.to_string());
        }
        let mut words = trimmed.split_whitespace();
        if let (Some(count), Some("failing")) = (words.next(), words.next()) {
            after_failing = count.parse::<usize>().is_ok();
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn known(check: &str, test: &str) -> DeliveryKnownFailure {
        DeliveryKnownFailure {
            check: check.into(),
            test: test.into(),
            reason: String::new(),
        }
    }

    #[test]
    fn a_check_is_settled_only_when_every_failure_is_known() {
        let check = "npm --prefix services/session-executor test";
        let marked = [
            known(check, "reads opened files through /proc"),
            known(check, "hub streams a session"),
        ];
        let mocha = |failing: &str, details: &str| {
            format!("  ✓ hub streams a session\n\n  40 passing\n  {failing} failing\n\n{details}")
        };
        // (output, known failures it settles on, by test)
        let cases: Vec<(String, Option<Vec<&str>>)> = vec![
            (
                mocha("1", "  1) filesystem\n       reads opened files through /proc:\n     Error"),
                Some(vec!["reads opened files through /proc"]),
            ),
            // A known test that passed this time does not cover another.
            (
                mocha("1", "  1) sessions\n       saves the turn:\n     Error"),
                None,
            ),
            (
                mocha(
                    "2",
                    "  1) filesystem\n       reads opened files through /proc:\n  2) sessions\n       saves the turn:",
                ),
                None,
            ),
            (
                " FAIL  test/hub.test.ts > hub streams a session\n Test Files  1 failed | 3 passed\n      Tests  1 failed | 30 passed".to_string(),
                Some(vec!["hub streams a session"]),
            ),
            (
                "test fs::reads opened files through /proc ... FAILED\ntest result: FAILED. 9 passed; 1 failed; 0 ignored".to_string(),
                Some(vec!["reads opened files through /proc"]),
            ),
            // No count to go by: it is not settled.
            ("Error: reads opened files through /proc".to_string(), None),
            // A known name inside a longer test's name is not that test.
            (
                " FAIL  test/hub.test.ts > hub streams a session forever\n      Tests  1 failed | 30 passed".to_string(),
                None,
            ),
        ];
        for (output, settles) in cases {
            let got = only_known(check, &output, &marked)
                .map(|found| found.iter().map(|failure| failure.test.as_str()).collect());
            assert_eq!(got, settles, "{output}");
        }
        // Nested known names count one failure once: an unknown second one
        // still fails the check.
        let nested = [
            known(check, "hub streams"),
            known(check, "hub streams a session"),
        ];
        let two = " FAIL  test/hub.test.ts > hub streams a session\n FAIL  test/x.test.ts > saves the turn\n      Tests  2 failed | 30 passed";
        assert!(only_known(check, two, &nested).is_none());
        // Another check's known failures are not this one's.
        let other = mocha(
            "1",
            "  1) filesystem\n       reads opened files through /proc:",
        );
        assert!(only_known("npm test", &other, &marked).is_none());
    }

    /// session-executor's `npm test` on Windows, as Tinto kept it (mocha's
    /// spec reporter, `--bail`), cut to the failing suite.
    const SESSION_EXECUTOR: &str = r#"
> session-executor@0.0.1 unit-tests
> mocha --config test/mocha/config.function.json

  Context images
    image type detection
      √ detects PNG
    LoadImage tool
      √ is offered as a read tool only to models that accept images
      1) tells apart images of the same size in its result


  96 passing (2s)
  1 failing

  1) Context images
       LoadImage tool
         tells apart images of the same size in its result:

      AssertionError [ERR_ASSERTION]: "ENOENT: no such file or directory, realpath 'C:\\proc\\self\\fd\\3'" != "ENOENT: no such file or directory, realpath 'C:\\proc\\self\\fd\\3'"
      + expected - actual


      at Context.<anonymous> (test\mocha\function-tests\test-context-images.ts:163:20)
"#;

    #[test]
    fn a_mocha_failure_is_known_by_its_title_or_its_suites_and_title() {
        let check = "npm --prefix services/session-executor test";
        for test in [
            "tells apart images of the same size in its result",
            "LoadImage tool tells apart images of the same size in its result",
            "Context images › LoadImage tool › tells apart images of the same size in its result",
        ] {
            let marked = [known(check, test)];
            assert!(
                only_known(check, SESSION_EXECUTOR, &marked).is_some(),
                "{test}"
            );
        }
        // Another failure next to it still fails the check, even when its
        // error names the known test.
        let two = SESSION_EXECUTOR.replace("1 failing", "2 failing")
            + "  2) Sessions\n       saves the turn:\n     Error: tells apart images of the same size in its result\n";
        let marked = [known(
            check,
            "LoadImage tool tells apart images of the same size in its result",
        )];
        assert!(only_known(check, &two, &marked).is_none());
    }
}
