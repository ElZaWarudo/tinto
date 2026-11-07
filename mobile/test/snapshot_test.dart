import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:tinto_delivery/src/screens.dart';
import 'package:tinto_delivery/src/snapshot.dart';

/// A snapshot as Tinto's mobile_link/snapshot.rs serializes it.
Snapshot sample() => Snapshot({
      'at_ms': 1,
      'runs': [
        {'id': 'r1', 'title': 'Lote de junio', 'paused': false}
      ],
      'tasks': [
        {
          'id': 't1',
          'run_id': 'r1',
          'key': 'K-1',
          'title': 'Exportar CSV',
          'phase': {'kind': 'approval', 'label': 'Aprobar PR', 'waits_on': 'user'},
          'pr': null
        },
        {
          'id': 't2',
          'run_id': 'r1',
          'key': 'K-2',
          'title': 'Filtro por fecha',
          'phase': {'kind': 'pr', 'label': 'PR #4: borrador', 'waits_on': 'user'},
          'pr': {
            'number': 4,
            'url': 'https://github.com/o/r/pull/4',
            'state': 'open',
            'draft': true,
            'conflicts': false,
            'ci': 'passing',
            'review_decision': null
          }
        },
        {
          'id': 't3',
          'run_id': 'r1',
          'key': 'K-3',
          'title': 'Otra',
          'phase': {'kind': 'running', 'label': 'En marcha: Tests', 'waits_on': 'agent'},
          'pr': null
        },
      ],
      'decisions': [
        {
          'id': 'd1',
          'task_id': 't1',
          'kind': 'choice',
          'question': '¿Separador?',
          'detail': '',
          'text': '',
          'command': null,
          'options': [
            {'label': 'Coma', 'consequence': 'Como Excel en inglés', 'recommended': true},
            {'label': 'Punto y coma', 'consequence': '', 'recommended': false},
          ],
          'recommendation': {'why': 'Es el estándar', 'basis': 'code', 'unknown': ''},
          'blocking': true
        }
      ],
      'approvals': [
        {
          'id': 'a1',
          'task_id': 't1',
          'rung': 'pr',
          'stage': null,
          'title': 'feat: exportar CSV',
          'body': 'Añade la exportación.\n\nCierra K-1.',
          'commits': [],
          'merge_method': null,
          'jira_statuses': [],
          'reviewers': ['ana'],
          'requested_at_ms': 1,
          'digest': 'abc',
          'truncated': false
        }
      ],
      'qa_problems': [],
      'settings_proposals': [],
      'failures': [],
    });

void main() {
  test('the inbox gathers what waits on the user, task by task', () {
    final inbox = sample().inbox();
    expect(inbox.map((i) => i.decision?.id ?? i.approval?.id ?? i.task.id), ['d1', 'a1', 't2']);
  });

  testWidgets('the inbox shows the exact text to publish and sends the digest with an approval',
      (tester) async {
    final sent = <Map<String, dynamic>>[];
    await tester.pumpWidget(MaterialApp(
      home: Scaffold(
        body: InboxView(
          snapshot: sample(),
          act: (action) async {
            sent.add(action);
            return null;
          },
          onOpen: (_) {},
        ),
      ),
    ));
    expect(find.text('Coma (recomendada)'), findsOneWidget);
    expect(find.text('Añade la exportación.\n\nCierra K-1.'), findsOneWidget);

    await tester.tap(find.text('Coma (recomendada)'));
    await tester.pump();
    expect(sent.last, {'action': 'answer_decision', 'decision_id': 'd1', 'answer': 'Coma'});

    final approve = find.widgetWithText(FilledButton, 'Aprobar');
    await tester.scrollUntilVisible(approve, 100, scrollable: find.byType(Scrollable).first);
    await tester.tap(approve);
    await tester.pumpAndSettle();
    await tester.tap(find.widgetWithText(FilledButton, 'Aprobar').last);
    await tester.pumpAndSettle();
    expect(sent.last, {
      'action': 'decide_approval',
      'approval_id': 'a1',
      'approve': true,
      'digest': 'abc',
    });
  });

  test('a PR reads as one line', () {
    expect(prLine(sample().task('t2')!.pr!), 'PR #4 borrador · CI en verde');
  });
}
