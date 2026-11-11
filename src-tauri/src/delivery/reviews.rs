//! Each reviewer of a followed PR as a small state machine. GitHub's list of
//! requested reviewers says only who it still waits on, and drops a reviewer
//! as soon as they review; what the PR waits on comes from what happened to
//! each reviewer, in order: asked, reviewed, answered, asked again.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Where a reviewer is.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReviewState {
    /// Asked for a review they have not given yet.
    Asked,
    /// Reviewed with comments and no verdict: they wait for an answer.
    Commented,
    /// Asked for changes: they wait for an answer, and GitHub blocks the
    /// merge until they approve or their review is dismissed.
    ChangesRequested,
    /// Got an answer to their review without being asked again.
    Answered,
    Approved,
}

/// What happens to a reviewer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewEvent {
    /// Asked for a review, from Tinto or on GitHub.
    Asked,
    /// A review: comments only, changes asked or an approval.
    Commented,
    ChangesRequested,
    Approved,
    /// Their approval or changes asked was dismissed on GitHub.
    Dismissed,
    /// The user's reply to their review was posted.
    Answered,
}

impl ReviewState {
    /// Where a reviewer goes on `event`; `None` when it changes nothing:
    /// a comment after an approval or a change request leaves the verdict
    /// standing, as GitHub does, and only a review can be answered.
    pub fn on(self, event: ReviewEvent) -> Option<Self> {
        use ReviewEvent as E;
        use ReviewState as S;
        match (self, event) {
            (_, E::Asked) => Some(S::Asked),
            (S::Asked | S::Commented | S::Answered, E::Commented) => Some(S::Commented),
            (_, E::ChangesRequested) => Some(S::ChangesRequested),
            (_, E::Approved) => Some(S::Approved),
            (S::Approved | S::ChangesRequested, E::Dismissed) => Some(S::Asked),
            (S::Commented | S::ChangesRequested, E::Answered) => Some(S::Answered),
            _ => None,
        }
    }

    /// The reviewer has to review (again).
    pub fn awaits_review(self) -> bool {
        matches!(self, Self::Asked | Self::Answered)
    }

    /// The reviewer waits for an answer to their review.
    pub fn awaits_answer(self) -> bool {
        matches!(self, Self::Commented | Self::ChangesRequested)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewerState {
    pub login: String,
    pub state: ReviewState,
    /// How many of their reviews Tinto took in, in GitHub's order: only
    /// later ones move them. A count rather than a time, since two reviews
    /// can share a second; GitHub never deletes a submitted review.
    #[serde(default)]
    pub reviews_read: usize,
}

/// Moves `login` on `event`. Asking someone new adds them; any other event
/// about someone who is not a reviewer (a commenter answered) is not theirs.
pub fn apply(states: &mut Vec<ReviewerState>, login: &str, event: ReviewEvent) {
    let at = match states
        .iter()
        .position(|known| known.login.eq_ignore_ascii_case(login))
    {
        Some(at) => at,
        None if event != ReviewEvent::Asked => return,
        None => {
            states.push(ReviewerState {
                login: login.to_string(),
                state: ReviewState::Asked,
                reviews_read: 0,
            });
            states.len() - 1
        }
    };
    if let Some(next) = states[at].state.on(event) {
        states[at].state = next;
    }
}

/// Brings the reviewers' states in line with a `gh pr view --json` answer
/// (with the REST `reviewComments` Tinto adds): one state per login in
/// `reviewers` (who must approve it); their reviews not taken in yet, in
/// order; a dismissed verdict; an answer of the user (`me`) on GitHub after
/// their latest review; and a request GitHub shows now, which is newer than
/// any review, since a review ends it.
pub fn follow(
    states: &mut Vec<ReviewerState>,
    reviewers: &[String],
    requested: &[String],
    snapshot: &Value,
    me: &str,
) {
    let same = |a: &str, b: &str| a.eq_ignore_ascii_case(b);
    states.retain(|state| reviewers.iter().any(|login| same(login, &state.login)));
    for login in reviewers {
        if !states.iter().any(|state| same(&state.login, login)) {
            states.push(ReviewerState {
                login: login.clone(),
                state: ReviewState::Asked,
                reviews_read: 0,
            });
        }
    }
    // (time, author, state) of every review, oldest first; a stable sort
    // keeps GitHub's order for reviews of the same second.
    let mut reviews: Vec<(&str, &str, &str)> = snapshot["reviews"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|review| {
            Some((
                review["submittedAt"].as_str()?,
                review["author"]["login"].as_str()?,
                review["state"].as_str()?,
            ))
        })
        .collect();
    reviews.sort_by(|a, b| a.0.cmp(b.0));
    let ours = |item: &Value, who: &str| {
        item[who]["login"]
            .as_str()
            .is_some_and(|author| same(author, me))
    };
    // Who opened the thread a review comment replies to: GitHub points
    // `in_reply_to_id` at the thread's first comment. `None` for a comment
    // that is not a reply, or whose first comment is not in the snapshot.
    let comments = snapshot["reviewComments"].as_array();
    let opener = |item: &Value| {
        let root = item["in_reply_to_id"].as_u64()?;
        comments
            .into_iter()
            .flatten()
            .find(|first| first["id"].as_u64() == Some(root))?["user"]["login"]
            .as_str()
    };
    // The user's replies in review threads, as (opener, time): each answers
    // only who opened that thread, so nobody when the user opened it.
    let replies: Vec<(&str, &str)> = comments
        .into_iter()
        .flatten()
        .filter(|item| ours(item, "user"))
        .filter_map(|item| Some((opener(item)?, item["created_at"].as_str()?)))
        .collect();
    // GitHub wraps the user's thread replies in a review of theirs with no
    // body and the replies' time (one review for a batch of them); gh gives
    // it no id to join on, so the time does. It is no answer to everyone.
    let wraps_replies = |item: &Value| {
        item["body"].as_str().is_some_and(str::is_empty)
            && item["submittedAt"]
                .as_str()
                .is_some_and(|at| replies.iter().any(|(_, time)| *time == at))
    };
    // When the user last wrote on the PR otherwise: a review, a comment, or
    // a review comment that is not such a reply (one whose thread is not in
    // the snapshot included). It answers every reviewer.
    let mine = [
        ("reviews", "submittedAt", "author"),
        ("comments", "createdAt", "author"),
        ("reviewComments", "created_at", "user"),
    ]
    .iter()
    .flat_map(|(list, time, who)| {
        snapshot[*list]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|item| ours(item, who) && opener(item).is_none() && !wraps_replies(item))
            .filter_map(|item| item[*time].as_str())
    })
    .max();
    for state in states.iter_mut() {
        let theirs: Vec<&(&str, &str, &str)> = reviews
            .iter()
            .filter(|(_, author, _)| same(author, &state.login))
            .collect();
        for (_, _, verdict) in theirs.iter().skip(state.reviews_read) {
            let event = match *verdict {
                "COMMENTED" => ReviewEvent::Commented,
                "CHANGES_REQUESTED" => ReviewEvent::ChangesRequested,
                "APPROVED" => ReviewEvent::Approved,
                // A dismissed review was a verdict that no longer stands.
                "DISMISSED" => ReviewEvent::Dismissed,
                _ => continue,
            };
            if let Some(next) = state.state.on(event) {
                state.state = next;
            }
        }
        state.reviews_read = theirs.len();
        // GitHub dismisses a verdict in place, so a later comment can follow
        // it: their latest verdict says whether it stands.
        let verdict = theirs
            .iter()
            .rev()
            .map(|(_, _, verdict)| *verdict)
            .find(|verdict| *verdict != "COMMENTED");
        if verdict == Some("DISMISSED") {
            if let Some(next) = state.state.on(ReviewEvent::Dismissed) {
                state.state = next;
            }
        }
        // The user answered on GitHub, by hand or through Tinto: in one of
        // their threads or to everyone.
        let last = theirs.last().map(|(time, _, _)| *time);
        let answer = replies
            .iter()
            .filter(|(opener, _)| same(opener, &state.login))
            .map(|(_, time)| *time)
            .chain(mine)
            .max();
        if answer.is_some_and(|answer| last.is_some_and(|last| answer > last)) {
            if let Some(next) = state.state.on(ReviewEvent::Answered) {
                state.state = next;
            }
        }
        if requested.iter().any(|login| same(login, &state.login)) {
            if let Some(next) = state.state.on(ReviewEvent::Asked) {
                state.state = next;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn review(login: &str, state: &str, at: &str) -> Value {
        json!({"author": {"login": login}, "state": state, "submittedAt": at})
    }

    fn state_of(states: &[ReviewerState], login: &str) -> ReviewState {
        states
            .iter()
            .find(|state| state.login == login)
            .unwrap()
            .state
    }

    #[test]
    fn a_reviewer_moves_through_review_answer_and_review_again() {
        let reviewers = vec!["ana".to_string()];
        let mut states = Vec::new();
        let mut follow = |requested: &[String], snapshot: Value| {
            follow(&mut states, &reviewers, requested, &snapshot, "me");
            state_of(&states, "ana")
        };
        assert_eq!(
            follow(&reviewers, json!({"reviews": []})),
            ReviewState::Asked
        );
        // GitHub drops the request once she reviews: she waits for an
        // answer, not "nobody asked"; reading it again moves nothing.
        let commented = review("ana", "COMMENTED", "2026-11-04T10:00:00Z");
        assert_eq!(
            follow(&[], json!({"reviews": [commented]})),
            ReviewState::Commented
        );
        assert_eq!(
            follow(&[], json!({"reviews": [commented]})),
            ReviewState::Commented
        );
        // The user answers in her thread on GitHub.
        let answer = json!({"user": {"login": "me"}, "created_at": "2026-11-04T11:00:00Z"});
        let answered = json!({"reviews": [commented], "reviewComments": [answer]});
        assert_eq!(follow(&[], answered.clone()), ReviewState::Answered);
        assert_eq!(follow(&reviewers, answered), ReviewState::Asked);
        let approved = review("ana", "APPROVED", "2026-11-04T12:00:00Z");
        assert_eq!(
            follow(&[], json!({"reviews": [commented, approved]})),
            ReviewState::Approved
        );
        // A comment after approving leaves the approval; a dismissal, which
        // GitHub makes in place, ends it.
        let later = review("ana", "COMMENTED", "2026-11-04T13:00:00Z");
        assert_eq!(
            follow(&[], json!({"reviews": [commented, approved, later]})),
            ReviewState::Approved
        );
        let dismissed = review("ana", "DISMISSED", "2026-11-04T12:00:00Z");
        assert_eq!(
            follow(&[], json!({"reviews": [commented, dismissed, later]})),
            ReviewState::Asked
        );
        // Taken off who must approve it: no state left.
        super::follow(&mut states, &[], &[], &json!({}), "me");
        assert!(states.is_empty());
    }

    #[test]
    fn reviews_of_the_same_second_keep_githubs_order_and_none_is_lost() {
        let reviewers = vec!["ana".to_string()];
        let mut states = Vec::new();
        let changes = review("ana", "CHANGES_REQUESTED", "2026-11-04T10:00:00Z");
        let approved = review("ana", "APPROVED", "2026-11-04T10:00:00Z");
        follow(
            &mut states,
            &reviewers,
            &[],
            &json!({"reviews": [changes]}),
            "me",
        );
        assert_eq!(state_of(&states, "ana"), ReviewState::ChangesRequested);
        follow(
            &mut states,
            &reviewers,
            &[],
            &json!({"reviews": [changes, approved]}),
            "me",
        );
        assert_eq!(state_of(&states, "ana"), ReviewState::Approved);
    }

    #[test]
    fn a_pr_followed_late_starts_from_its_whole_review_history() {
        let reviewers = vec!["ana".to_string(), "luis".to_string()];
        let mut states = Vec::new();
        let snapshot = json!({"reviews": [
            review("me", "COMMENTED", "2026-11-03T08:00:00Z"),
            review("luis", "CHANGES_REQUESTED", "2026-11-03T09:00:00Z"),
            review("ana", "APPROVED", "2026-11-03T10:00:00Z"),
            review("luis", "COMMENTED", "2026-11-03T11:00:00Z"),
        ]});
        follow(&mut states, &reviewers, &[], &snapshot, "me");
        assert_eq!(state_of(&states, "ana"), ReviewState::Approved);
        assert_eq!(state_of(&states, "luis"), ReviewState::ChangesRequested);
        assert_eq!(states.len(), 2);
    }

    #[test]
    fn a_reply_in_a_thread_answers_only_who_opened_it() {
        let reviewers = vec!["ana".to_string(), "luis".to_string()];
        let mut states = Vec::new();
        let comment = |id: u64, reply_to: Option<u64>, login: &str, at: &str| {
            json!({"id": id, "in_reply_to_id": reply_to, "user": {"login": login},
                   "created_at": at})
        };
        let mut snapshot = json!({
            "reviews": [
                review("ana", "COMMENTED", "2026-11-04T10:00:00Z"),
                review("luis", "COMMENTED", "2026-11-04T10:30:00Z"),
                // The review GitHub wraps the user's reply in.
                {"author": {"login": "me"}, "state": "COMMENTED", "body": "",
                 "submittedAt": "2026-11-04T11:00:00Z"},
            ],
            "reviewComments": [
                comment(1, None, "ana", "2026-11-04T10:00:00Z"),
                comment(2, Some(1), "me", "2026-11-04T11:00:00Z"),
            ],
        });
        follow(&mut states, &reviewers, &[], &snapshot, "me");
        assert_eq!(state_of(&states, "ana"), ReviewState::Answered);
        assert_eq!(state_of(&states, "luis"), ReviewState::Commented);
        // A general comment answers everyone.
        snapshot["comments"] =
            json!([{"author": {"login": "me"}, "createdAt": "2026-11-04T12:00:00Z"}]);
        follow(&mut states, &reviewers, &[], &snapshot, "me");
        assert_eq!(state_of(&states, "luis"), ReviewState::Answered);
    }
}
