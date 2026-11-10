// The pieces the screens are made of: a task's phase, and one card per kind
// of pending item, each with the actions Tinto allows from the phone.

import 'package:flutter/material.dart';

import 'snapshot.dart';

/// Sends an action to Tinto; resolves to null once done, else why not.
typedef Act = Future<String?> Function(Map<String, dynamic> action);

enum Tone { attention, working, danger, done, idle }

const _brokenKinds = {'broken', 'setup_failed', 'merged_failing_ci', 'drifted', 'quarantined'};

Tone phaseTone(TaskPhase? phase) {
  if (phase == null) return Tone.idle;
  if (_brokenKinds.contains(phase.kind)) return Tone.danger;
  if (phase.needsYou) return Tone.attention;
  if (phase.kind == 'delivered') return Tone.done;
  if (phase.waitsOn == 'agent' || phase.kind == 'step_running') return Tone.working;
  return Tone.idle;
}

Color toneColor(Tone tone, ColorScheme scheme) => switch (tone) {
      Tone.attention => Colors.orange.shade800,
      Tone.working => scheme.primary,
      Tone.danger => scheme.error,
      Tone.done => Colors.green.shade700,
      Tone.idle => scheme.outline,
    };

class PhaseBadge extends StatelessWidget {
  const PhaseBadge(this.phase, {super.key});
  final TaskPhase? phase;

  @override
  Widget build(BuildContext context) {
    final color = toneColor(phaseTone(phase), Theme.of(context).colorScheme);
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 2),
      decoration: BoxDecoration(
        color: color.withOpacity(0.12),
        border: Border.all(color: color),
        borderRadius: BorderRadius.circular(12),
      ),
      child: Text(phase?.label ?? 'Sin fase', style: TextStyle(color: color, fontSize: 12)),
    );
  }
}

/// Runs an action and says how it ended.
Future<void> runAction(BuildContext context, Act act, Map<String, dynamic> action) async {
  final messenger = ScaffoldMessenger.of(context);
  final error = await act(action);
  messenger.showSnackBar(SnackBar(content: Text(error ?? 'Hecho.')));
}

class _Mono extends StatelessWidget {
  const _Mono(this.text);
  final String text;

  @override
  Widget build(BuildContext context) => Container(
        width: double.infinity,
        margin: const EdgeInsets.symmetric(vertical: 4),
        padding: const EdgeInsets.all(8),
        decoration: BoxDecoration(
          color: Theme.of(context).colorScheme.surfaceVariant,
          borderRadius: BorderRadius.circular(6),
        ),
        child: SelectableText(text, style: const TextStyle(fontFamily: 'monospace', fontSize: 13)),
      );
}

class _ItemCard extends StatelessWidget {
  const _ItemCard({required this.task, required this.heading, required this.children});
  final Task task;
  final String heading;
  final List<Widget> children;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Card(
      margin: const EdgeInsets.symmetric(horizontal: 12, vertical: 6),
      child: Padding(
        padding: const EdgeInsets.all(12),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text('${task.key} · ${task.title}', style: theme.textTheme.labelMedium),
            const SizedBox(height: 4),
            Text(heading, style: theme.textTheme.titleMedium),
            const SizedBox(height: 4),
            ...children,
          ],
        ),
      ),
    );
  }
}

class DecisionCard extends StatelessWidget {
  const DecisionCard({super.key, required this.task, required this.decision, required this.act});
  final Task task;
  final Decision decision;
  final Act act;

  void _answer(BuildContext context, String answer) => runAction(
      context, act, {'action': 'answer_decision', 'decision_id': decision.id, 'answer': answer});

  Widget _optionButton(BuildContext context, DecisionOption option) {
    final label = Padding(
      padding: const EdgeInsets.symmetric(vertical: 6),
      child: Column(children: [
        Text(option.recommended ? '${option.label} (recomendada)' : option.label),
        if (option.consequence.isNotEmpty)
          Text(option.consequence, style: const TextStyle(fontSize: 12)),
      ]),
    );
    void answer() => _answer(context, option.label);
    return option.recommended
        ? FilledButton(onPressed: answer, child: label)
        : OutlinedButton(onPressed: answer, child: label);
  }

  @override
  Widget build(BuildContext context) {
    final d = decision;
    return _ItemCard(task: task, heading: d.question, children: [
      if (!d.blocking) const Text('Ya decidida con la recomendación; puedes cambiarla en Tinto.'),
      if (d.detail.isNotEmpty)
        ExpansionTile(
          title: const Text('Detalle'),
          tilePadding: EdgeInsets.zero,
          children: [Text(d.detail)],
        ),
      if (d.kind == 'choice') ...[
        if (d.recommendationWhy != null) Text('Recomendada porque: ${d.recommendationWhy}'),
        for (final option in d.options)
          Padding(
            padding: const EdgeInsets.only(top: 6),
            child: SizedBox(width: double.infinity, child: _optionButton(context, option)),
          ),
      ],
      if (d.kind == 'text') ...[
        const Text('El texto, tal como lo verán:'),
        _Mono(d.text),
        FilledButton(onPressed: () => _answer(context, d.text), child: const Text('Aprobar el texto')),
      ],
      if (d.kind == 'permission') ...[
        if (d.command != null) _Mono(d.command!),
        if (d.inTinto)
          Text('Concede más de lo que el móvil muestra: permítelo en Tinto.',
              style: TextStyle(color: Theme.of(context).colorScheme.error)),
        Row(children: [
          FilledButton(
            onPressed: d.inTinto ? null : () => _answer(context, 'allowed'),
            child: const Text('Permitir'),
          ),
          const SizedBox(width: 8),
          OutlinedButton(onPressed: () => _answer(context, 'denied'), child: const Text('Denegar')),
        ]),
      ],
    ]);
  }
}

class ApprovalCard extends StatelessWidget {
  const ApprovalCard({super.key, required this.task, required this.approval, required this.act});
  final Task task;
  final Approval approval;
  final Act act;

  Future<void> _approve(BuildContext context) async {
    final sure = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: Text('¿Aprobar: ${approval.step}?'),
        content: const Text('Tinto lo hará ahora con el texto que ves.'),
        actions: [
          TextButton(onPressed: () => Navigator.pop(context, false), child: const Text('Cancelar')),
          FilledButton(onPressed: () => Navigator.pop(context, true), child: const Text('Aprobar')),
        ],
      ),
    );
    if (sure == true && context.mounted) _decide(context, true);
  }

  void _decide(BuildContext context, bool approve) => runAction(context, act, {
        'action': 'decide_approval',
        'approval_id': approval.id,
        'approve': approve,
        'digest': approval.digest,
      });

  @override
  Widget build(BuildContext context) {
    final a = approval;
    return _ItemCard(task: task, heading: 'Aprobar: ${a.step}', children: [
      if (a.commits.isNotEmpty) ...[
        Text(a.commits.length == 1 ? 'El commit:' : 'Los ${a.commits.length} commits:'),
        for (final message in a.commits) _Mono(message),
      ] else ...[
        if (a.title.isNotEmpty) _Mono(a.title),
        if (a.body.isNotEmpty) _Mono(a.body),
      ],
      if (a.mergeMethod != null) Text('Método: ${a.mergeMethod}'),
      if (a.jiraStatuses.isNotEmpty) Text('Estados de Jira: ${a.jiraStatuses.join(' → ')}'),
      if (a.reviewers.isNotEmpty) Text('Revisores: ${a.reviewers.join(', ')}'),
      if (a.rung == 'pr' && a.reviewers.isEmpty)
        const Text('Sin revisores: para elegirlos, apruébalo en Tinto.'),
      if (a.truncated)
        Text('El texto no cabe entero en el móvil: apruébalo en Tinto.',
            style: TextStyle(color: Theme.of(context).colorScheme.error)),
      const SizedBox(height: 6),
      Row(children: [
        FilledButton(
          onPressed: a.truncated ? null : () => _approve(context),
          child: const Text('Aprobar'),
        ),
        const SizedBox(width: 8),
        OutlinedButton(onPressed: () => _decide(context, false), child: const Text('Rechazar')),
      ]),
    ]);
  }
}

class QaProblemCard extends StatefulWidget {
  const QaProblemCard({super.key, required this.task, required this.problem, required this.act});
  final Task task;
  final QaProblem problem;
  final Act act;

  @override
  State<QaProblemCard> createState() => _QaProblemCardState();
}

class _QaProblemCardState extends State<QaProblemCard> {
  int _picked = 0;
  final _own = TextEditingController();

  @override
  void dispose() {
    _own.dispose();
    super.dispose();
  }

  String get _solution {
    final own = _own.text.trim();
    if (own.isNotEmpty) return own;
    final suggestions = widget.problem.suggestions;
    return _picked < suggestions.length ? suggestions[_picked] : '';
  }

  @override
  Widget build(BuildContext context) {
    final p = widget.problem;
    return _ItemCard(task: widget.task, heading: 'Problema de QA #${p.number} ${p.label}', children: [
      if (p.place.isNotEmpty) Text(p.place, style: Theme.of(context).textTheme.bodySmall),
      Text(p.text),
      for (var i = 0; i < p.suggestions.length; i++)
        RadioListTile<int>(
          value: i,
          groupValue: _picked,
          contentPadding: EdgeInsets.zero,
          title: Text(p.suggestions[i]),
          onChanged: (value) => setState(() => _picked = value ?? 0),
        ),
      TextField(
        controller: _own,
        decoration: const InputDecoration(labelText: 'Otra solución (opcional)'),
        onChanged: (_) => setState(() {}),
      ),
      const SizedBox(height: 6),
      FilledButton(
        onPressed: _solution.isEmpty
            ? null
            : () => runAction(context, widget.act, {
                  'action': 'send_qa_problems',
                  'task_id': p.taskId,
                  'choices': [
                    {'problem_id': p.id, 'solution': _solution}
                  ],
                }),
        child: const Text('Enviar a corregir'),
      ),
    ]);
  }
}

class InboxCard extends StatelessWidget {
  const InboxCard({super.key, required this.item, required this.act, this.onOpen});
  final InboxItem item;
  final Act act;
  final VoidCallback? onOpen;

  @override
  Widget build(BuildContext context) {
    final item = this.item;
    if (item.decision != null) {
      return DecisionCard(task: item.task, decision: item.decision!, act: act);
    }
    if (item.approval != null) {
      return ApprovalCard(task: item.task, approval: item.approval!, act: act);
    }
    if (item.qaProblem != null) {
      return QaProblemCard(task: item.task, problem: item.qaProblem!, act: act);
    }
    if (item.proposal != null) {
      return _ItemCard(task: item.task, heading: 'Propuesta de ajustes', children: [
        Text(item.proposal!.reason),
        const Text('Revísala en Tinto: cambia los ajustes del repositorio.'),
      ]);
    }
    return Card(
      margin: const EdgeInsets.symmetric(horizontal: 12, vertical: 6),
      child: TaskTile(task: item.task, onTap: onOpen),
    );
  }
}

class TaskTile extends StatelessWidget {
  const TaskTile({super.key, required this.task, this.onTap});
  final Task task;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) => ListTile(
        title: Text('${task.key} · ${task.title}', maxLines: 2, overflow: TextOverflow.ellipsis),
        subtitle: Align(alignment: Alignment.centerLeft, child: PhaseBadge(task.phase)),
        trailing: task.pr == null ? null : Text('PR #${task.pr!.number}'),
        onTap: onTap,
      );
}
