//! Tests the user marked as failing only because of the environment (a
//! Windows run without /proc, a flaky gRPC test): a check whose every
//! failure is one of them does not send the work back, while any other
//! failure still does.

use super::model::DeliveryKnownFailure;

/// The known failures `output` of check `check` shows, when they are all it
/// failed on: its failure count (mocha's "N failing", jest's or vitest's
/// "N failed", cargo's "test result: FAILED. … N failed", summed) is known
/// and each failure is one of them, named where the output shows failures
/// (a line marked ✕, ×, ✗, ● or FAIL, or anything after mocha's "N
/// failing"). `None` when any failure is not known, or the count is not
/// found.
pub fn only_known<'a>(
    check: &str,
    output: &str,
    known: &'a [DeliveryKnownFailure],
) -> Option<Vec<&'a DeliveryKnownFailure>> {
    let count = failure_count(output)?;
    let failing = failure_lines(output);
    let mut matched: Vec<&DeliveryKnownFailure> = Vec::new();
    for failure in known {
        let test = failure.test.trim();
        if failure.check.trim() == check.trim()
            && !test.is_empty()
            && failing.iter().any(|line| line.contains(test))
            && !matched.iter().any(|seen| seen.test.trim() == test)
        {
            matched.push(failure);
        }
    }
    (count > 0 && matched.len() >= count).then_some(matched)
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

/// Lines that show a failing test.
fn failure_lines(output: &str) -> Vec<&str> {
    let mut lines = Vec::new();
    let mut after_failing = false;
    for line in output.lines() {
        let trimmed = line.trim();
        let marked = ["✕", "×", "✗", "●", "FAIL"]
            .iter()
            .any(|mark| trimmed.contains(mark));
        if after_failing || marked {
            lines.push(line);
        }
        let mut words = trimmed.split_whitespace();
        if let (Some(count), Some("failing")) = (words.next(), words.next()) {
            after_failing |= count.parse::<usize>().is_ok();
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
        ];
        for (output, settles) in cases {
            let got = only_known(check, &output, &marked)
                .map(|found| found.iter().map(|failure| failure.test.as_str()).collect());
            assert_eq!(got, settles, "{output}");
        }
        // Another check's known failures are not this one's.
        let other = mocha(
            "1",
            "  1) filesystem\n       reads opened files through /proc:",
        );
        assert!(only_known("npm test", &other, &marked).is_none());
    }
}
