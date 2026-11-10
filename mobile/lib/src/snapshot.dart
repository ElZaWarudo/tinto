// The snapshot Tinto sends (docs/contracts/mobile-link.md §5) and what the
// screens derive from it.

String _s(Object? value) => value is String ? value : '';
String? _opt(Object? value) => value is String ? value : null;
List<Map<String, dynamic>> _list(Object? value) =>
    value is List ? value.whereType<Map<String, dynamic>>().toList() : const [];
List<String> _strings(Object? value) =>
    value is List ? value.whereType<String>().toList() : const [];

class Run {
  Run(Map<String, dynamic> j)
      : id = _s(j['id']),
        title = _s(j['title']),
        paused = j['paused'] == true;
  final String id, title;
  final bool paused;
}

class TaskPhase {
  TaskPhase(Map<String, dynamic> j)
      : kind = _s(j['kind']),
        label = _s(j['label']),
        waitsOn = _s(j['waits_on']);
  final String kind, label, waitsOn;
  bool get needsYou => waitsOn == 'user';
}

class Pr {
  Pr(Map<String, dynamic> j)
      : number = (j['number'] as num?)?.toInt() ?? 0,
        url = _s(j['url']),
        state = _s(j['state']),
        draft = j['draft'] == true,
        conflicts = j['conflicts'] == true,
        ci = _opt(j['ci']),
        reviewDecision = _opt(j['review_decision']);
  final int number;
  final String url, state;
  final bool draft, conflicts;
  final String? ci, reviewDecision;
}

class Task {
  Task(Map<String, dynamic> j)
      : id = _s(j['id']),
        runId = _opt(j['run_id']),
        key = _s(j['key']),
        title = _s(j['title']),
        phase = j['phase'] is Map<String, dynamic> ? TaskPhase(j['phase']) : null,
        pr = j['pr'] is Map<String, dynamic> ? Pr(j['pr']) : null;
  final String id, key, title;
  final String? runId;
  final TaskPhase? phase;
  final Pr? pr;
}

class DecisionOption {
  DecisionOption(Map<String, dynamic> j)
      : label = _s(j['label']),
        consequence = _s(j['consequence']),
        recommended = j['recommended'] == true;
  final String label, consequence;
  final bool recommended;
}

class Decision {
  Decision(Map<String, dynamic> j)
      : id = _s(j['id']),
        taskId = _s(j['task_id']),
        kind = _s(j['kind']),
        question = _s(j['question']),
        detail = _s(j['detail']),
        text = _s(j['text']),
        command = _opt(j['command']),
        options = _list(j['options']).map(DecisionOption.new).toList(),
        recommendationWhy = j['recommendation'] is Map ? _s(j['recommendation']['why']) : null,
        blocking = j['blocking'] != false,
        inTinto = j['in_tinto'] == true;
  final String id, taskId, kind, question, detail, text;
  final String? command, recommendationWhy;
  final List<DecisionOption> options;
  final bool blocking;

  /// The phone does not show all it would allow or approve: only Tinto can.
  final bool inTinto;
}

class Approval {
  Approval(Map<String, dynamic> j)
      : id = _s(j['id']),
        taskId = _s(j['task_id']),
        rung = _s(j['rung']),
        stage = _opt(j['stage']),
        title = _s(j['title']),
        body = _s(j['body']),
        commits = _strings(j['commits']),
        mergeMethod = _opt(j['merge_method']),
        jiraStatuses = _strings(j['jira_statuses']),
        reviewers = _strings(j['reviewers']),
        digest = _s(j['digest']),
        truncated = j['truncated'] == true;
  final String id, taskId, rung, title, body, digest;
  final String? stage, mergeMethod;
  final List<String> commits, jiraStatuses, reviewers;
  final bool truncated;

  /// What approving does, in the words of Tinto's own buttons.
  String get step => switch (rung) {
        'commit' => 'Commit',
        'sync' => 'Sincronizar con la base',
        'push' => 'Push',
        'pr' => 'Abrir el PR',
        'merge' => 'Fusionar',
        'jira' => stage == 'after_merge' ? 'Jira (merge)' : 'Jira (PR)',
        'pr_reply' => 'Respuesta al revisor',
        _ => rung,
      };
}

class QaProblem {
  QaProblem(Map<String, dynamic> j)
      : id = _s(j['id']),
        taskId = _s(j['task_id']),
        number = (j['number'] as num?)?.toInt() ?? 0,
        label = _s(j['label']),
        place = _s(j['place']),
        text = _s(j['text']),
        suggestions = _strings(j['suggestions']);
  final String id, taskId, label, place, text;
  final int number;
  final List<String> suggestions;
}

class SettingsProposal {
  SettingsProposal(Map<String, dynamic> j)
      : id = _s(j['id']),
        taskId = _s(j['task_id']),
        reason = _s(j['reason']);
  final String id, taskId, reason;
}

class Failure {
  Failure(Map<String, dynamic> j)
      : taskId = _s(j['task_id']),
        role = _s(j['role']),
        error = _s(j['error']);
  final String taskId, role, error;
}

/// Something pending in the inbox, about one task.
class InboxItem {
  InboxItem({required this.task, this.decision, this.approval, this.qaProblem, this.proposal});
  final Task task;
  final Decision? decision;
  final Approval? approval;
  final QaProblem? qaProblem;
  final SettingsProposal? proposal;
}

class Snapshot {
  Snapshot(Map<String, dynamic> j)
      : runs = _list(j['runs']).map(Run.new).toList(),
        tasks = _list(j['tasks']).map(Task.new).toList(),
        decisions = _list(j['decisions']).map(Decision.new).toList(),
        approvals = _list(j['approvals']).map(Approval.new).toList(),
        qaProblems = _list(j['qa_problems']).map(QaProblem.new).toList(),
        proposals = _list(j['settings_proposals']).map(SettingsProposal.new).toList(),
        failures = _list(j['failures']).map(Failure.new).toList(),
        partial = j['partial'] == true;

  final List<Run> runs;
  final List<Task> tasks;
  final List<Decision> decisions;
  final List<Approval> approvals;
  final List<QaProblem> qaProblems;
  final List<SettingsProposal> proposals;
  final List<Failure> failures;
  final bool partial;

  Task? task(String id) => tasks.where((t) => t.id == id).firstOrNull;

  List<Task> tasksOf(String? runId) => tasks.where((t) => t.runId == runId).toList();

  /// Everything that waits on the user: blocking decisions first, then
  /// approvals, QA problems, settings proposals, and the tasks that wait on
  /// the user for something else (a PR draft, a reply).
  List<InboxItem> inbox() {
    final items = <InboxItem>[];
    void add(String taskId, InboxItem Function(Task task) make) {
      final task = this.task(taskId);
      if (task != null) items.add(make(task));
    }

    final blockingFirst = [
      ...decisions.where((d) => d.blocking),
      ...decisions.where((d) => !d.blocking),
    ];
    for (final d in blockingFirst) {
      add(d.taskId, (t) => InboxItem(task: t, decision: d));
    }
    for (final a in approvals) {
      add(a.taskId, (t) => InboxItem(task: t, approval: a));
    }
    for (final p in qaProblems) {
      add(p.taskId, (t) => InboxItem(task: t, qaProblem: p));
    }
    for (final p in proposals) {
      add(p.taskId, (t) => InboxItem(task: t, proposal: p));
    }
    final covered = items.map((item) => item.task.id).toSet();
    for (final t in tasks) {
      if (t.phase?.needsYou == true && !covered.contains(t.id)) items.add(InboxItem(task: t));
    }
    return items;
  }
}
