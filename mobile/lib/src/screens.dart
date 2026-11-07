// The app's screens: pairing, the inbox and batches, and one task.

import 'package:flutter/material.dart';
import 'package:mobile_scanner/mobile_scanner.dart';

import 'link.dart';
import 'pairing.dart';
import 'snapshot.dart';
import 'widgets.dart';

class PairScreen extends StatefulWidget {
  const PairScreen({super.key, required this.onPaired});
  final ValueChanged<Pairing> onPaired;

  @override
  State<PairScreen> createState() => _PairScreenState();
}

class _PairScreenState extends State<PairScreen> {
  final _device = TextEditingController(text: 'Mi móvil');
  final _link = TextEditingController();
  String? _error;

  @override
  void dispose() {
    _device.dispose();
    _link.dispose();
    super.dispose();
  }

  void _pair(String link) {
    final pairing = Pairing.parse(link, device: _device.text);
    if (pairing == null) {
      setState(() => _error = 'Ese enlace no es un código de Tinto.');
      return;
    }
    widget.onPaired(pairing);
  }

  Future<void> _scan() async {
    final link = await Navigator.push<String>(
      context,
      MaterialPageRoute(builder: (_) => const ScanScreen()),
    );
    if (link != null) _pair(link);
  }

  @override
  Widget build(BuildContext context) => Scaffold(
        appBar: AppBar(title: const Text('Vincular con Tinto')),
        body: ListView(padding: const EdgeInsets.all(16), children: [
          const Text('En Tinto, abre Ajustes → Delivery → Móvil y pulsa «Vincular móvil». '
              'Escanea el código o pega el enlace. Después confirma este móvil en Tinto.'),
          const SizedBox(height: 16),
          TextField(
            controller: _device,
            decoration: const InputDecoration(labelText: 'Nombre de este móvil (lo verás en Tinto)'),
          ),
          const SizedBox(height: 16),
          FilledButton.icon(
            onPressed: _scan,
            icon: const Icon(Icons.qr_code_scanner),
            label: const Text('Escanear el código QR'),
          ),
          const SizedBox(height: 24),
          TextField(
            controller: _link,
            decoration: const InputDecoration(labelText: 'O pega el enlace tinto-link://…'),
          ),
          const SizedBox(height: 8),
          OutlinedButton(onPressed: () => _pair(_link.text), child: const Text('Vincular')),
          if (_error != null)
            Padding(
              padding: const EdgeInsets.only(top: 8),
              child: Text(_error!, style: TextStyle(color: Theme.of(context).colorScheme.error)),
            ),
        ]),
      );
}

class ScanScreen extends StatefulWidget {
  const ScanScreen({super.key});

  @override
  State<ScanScreen> createState() => _ScanScreenState();
}

class _ScanScreenState extends State<ScanScreen> {
  bool _done = false;

  @override
  Widget build(BuildContext context) => Scaffold(
        appBar: AppBar(title: const Text('Escanear el código')),
        body: MobileScanner(
          onDetect: (capture) {
            final value = capture.barcodes.map((b) => b.rawValue).whereType<String>().firstOrNull;
            if (_done || value == null || !value.startsWith('tinto-link://')) return;
            _done = true;
            Navigator.pop(context, value);
          },
        ),
      );
}

String _connection(LinkState state) => switch (state) {
      LinkState.offline => 'Sin conexión',
      LinkState.connecting => 'Conectando…',
      LinkState.waitingDesktop => 'Esperando a Tinto',
      LinkState.pending => 'Pendiente de confirmar',
      LinkState.paired => 'Conectado',
      LinkState.revoked => 'Revocado',
    };

class HomeScreen extends StatefulWidget {
  const HomeScreen({super.key, required this.link, required this.onUnpair});
  final LinkClient link;
  final VoidCallback onUnpair;

  @override
  State<HomeScreen> createState() => _HomeScreenState();
}

class _HomeScreenState extends State<HomeScreen> {
  int _tab = 0;

  void _open(String taskId) => Navigator.push(
        context,
        MaterialPageRoute(builder: (_) => TaskScreen(link: widget.link, taskId: taskId)),
      );

  @override
  Widget build(BuildContext context) => ListenableBuilder(
        listenable: widget.link,
        builder: (context, _) {
          final link = widget.link;
          final snapshot = link.snapshot;
          final inbox = snapshot?.inbox() ?? const [];
          final online = link.state == LinkState.paired;
          return Scaffold(
            appBar: AppBar(
              title: const Text('Tinto Delivery'),
              actions: [
                Icon(Icons.circle, size: 12, color: online ? Colors.green : Colors.grey),
                Padding(
                  padding: const EdgeInsets.symmetric(horizontal: 8),
                  child: Center(child: Text(_connection(link.state))),
                ),
                PopupMenuButton<String>(
                  onSelected: (_) => widget.onUnpair(),
                  itemBuilder: (_) => const [
                    PopupMenuItem(value: 'unpair', child: Text('Desvincular este móvil')),
                  ],
                ),
              ],
            ),
            body: _body(link, snapshot, inbox),
            bottomNavigationBar: NavigationBar(
              selectedIndex: _tab,
              onDestinationSelected: (tab) => setState(() => _tab = tab),
              destinations: [
                NavigationDestination(
                  icon: Badge(
                    isLabelVisible: inbox.isNotEmpty,
                    label: Text('${inbox.length}'),
                    child: const Icon(Icons.inbox),
                  ),
                  label: 'Te necesita',
                ),
                const NavigationDestination(icon: Icon(Icons.view_list), label: 'Lotes'),
              ],
            ),
          );
        },
      );

  Widget _body(LinkClient link, Snapshot? snapshot, List<InboxItem> inbox) {
    Widget message(String text, {Widget? action}) => Center(
          child: Padding(
            padding: const EdgeInsets.all(24),
            child: Column(mainAxisSize: MainAxisSize.min, children: [
              Text(text, textAlign: TextAlign.center),
              if (action != null) ...[const SizedBox(height: 16), action],
            ]),
          ),
        );
    if (link.state == LinkState.revoked) {
      return message('Tinto revocó este móvil.',
          action: FilledButton(onPressed: widget.onUnpair, child: const Text('Vincular de nuevo')));
    }
    if (link.state == LinkState.pending) {
      return message('Confirma «${link.pairing.device}» en Tinto (Ajustes → Delivery → Móvil).');
    }
    if (snapshot == null) {
      return message(link.error ?? 'Esperando a Tinto… Tinto tiene que estar abierto.');
    }
    final notice = <Widget>[
      if (link.state != LinkState.paired)
        const MaterialBanner(
          content: Text('Sin conexión con Tinto: lo que ves puede no estar al día.'),
          actions: [SizedBox.shrink()],
        ),
      if (snapshot.partial)
        const MaterialBanner(
          content: Text('Hay demasiado pendiente para el móvil: ábrelo en Tinto.'),
          actions: [SizedBox.shrink()],
        ),
    ];
    final act = link.act;
    return Column(children: [
      ...notice,
      Expanded(
        child: _tab == 0
            ? InboxView(snapshot: snapshot, act: act, onOpen: _open)
            : BatchesView(snapshot: snapshot, act: act, onOpen: _open),
      ),
    ]);
  }
}

class InboxView extends StatelessWidget {
  const InboxView({super.key, required this.snapshot, required this.act, required this.onOpen});
  final Snapshot snapshot;
  final Act act;
  final ValueChanged<String> onOpen;

  @override
  Widget build(BuildContext context) {
    final inbox = snapshot.inbox();
    if (inbox.isEmpty) return const Center(child: Text('Nada te espera ahora.'));
    return ListView(
      padding: const EdgeInsets.symmetric(vertical: 8),
      children: [
        for (final item in inbox)
          InboxCard(item: item, act: act, onOpen: () => onOpen(item.task.id)),
      ],
    );
  }
}

class BatchesView extends StatelessWidget {
  const BatchesView({super.key, required this.snapshot, required this.act, required this.onOpen});
  final Snapshot snapshot;
  final Act act;
  final ValueChanged<String> onOpen;

  @override
  Widget build(BuildContext context) {
    final loose = snapshot.tasksOf(null);
    return ListView(children: [
      for (final run in snapshot.runs) ...[
        ListTile(
          title: Text(run.title, style: Theme.of(context).textTheme.titleMedium),
          subtitle: run.paused ? const Text('En pausa') : null,
          trailing: IconButton(
            tooltip: run.paused ? 'Reanudar el lote' : 'Pausar el lote',
            icon: Icon(run.paused ? Icons.play_arrow : Icons.pause),
            onPressed: () => runAction(
                context, act, {'action': 'set_run_paused', 'run_id': run.id, 'paused': !run.paused}),
          ),
        ),
        for (final task in snapshot.tasksOf(run.id))
          TaskTile(task: task, onTap: () => onOpen(task.id)),
        const Divider(),
      ],
      if (loose.isNotEmpty) ...[
        const ListTile(title: Text('Sin lote')),
        for (final task in loose) TaskTile(task: task, onTap: () => onOpen(task.id)),
      ],
      if (snapshot.runs.isEmpty && loose.isEmpty)
        const Padding(padding: EdgeInsets.all(24), child: Text('No hay lotes activos.')),
    ]);
  }
}

String prLine(Pr pr) {
  final state = switch (pr.state) {
    'open' => pr.draft ? 'borrador' : 'abierto',
    'merged' => 'fusionado',
    'closed' => 'cerrado',
    _ => 'sin leer',
  };
  final ci = switch (pr.ci) {
    'passing' => 'CI en verde',
    'failing' => 'CI en rojo',
    'pending' => 'CI en curso',
    'not_run' => 'CI sin ejecutar',
    _ => null,
  };
  final review = switch (pr.reviewDecision) {
    'approved' => 'aprobado',
    'changes_requested' => 'cambios pedidos',
    'review_required' => 'revisión pendiente',
    _ => null,
  };
  return [
    'PR #${pr.number} $state',
    if (ci != null) ci,
    if (review != null) review,
    if (pr.conflicts) 'con conflictos',
  ].join(' · ');
}

class TaskScreen extends StatelessWidget {
  const TaskScreen({super.key, required this.link, required this.taskId});
  final LinkClient link;
  final String taskId;

  @override
  Widget build(BuildContext context) => ListenableBuilder(
        listenable: link,
        builder: (context, _) {
          final snapshot = link.snapshot;
          final task = snapshot?.task(taskId);
          if (snapshot == null || task == null) {
            return Scaffold(
              appBar: AppBar(),
              body: const Center(child: Text('Esta tarea ya no está entre las activas.')),
            );
          }
          final items = snapshot.inbox().where((item) => item.task.id == taskId);
          final recommended = snapshot.decisions.any((d) =>
              d.taskId == taskId && d.kind == 'choice' && d.options.any((o) => o.recommended));
          final failures = snapshot.failures.where((f) => f.taskId == taskId);
          return Scaffold(
            appBar: AppBar(title: Text(task.key)),
            body: ListView(padding: const EdgeInsets.symmetric(vertical: 8), children: [
              ListTile(title: Text(task.title), subtitle: Align(
                alignment: Alignment.centerLeft,
                child: PhaseBadge(task.phase),
              )),
              if (task.pr != null) ListTile(leading: const Icon(Icons.merge), title: Text(prLine(task.pr!))),
              for (final failure in failures)
                ListTile(
                  leading: Icon(Icons.error_outline, color: Theme.of(context).colorScheme.error),
                  title: Text('Falló ${failure.role}'),
                  subtitle: Text(failure.error),
                ),
              if (recommended)
                Padding(
                  padding: const EdgeInsets.symmetric(horizontal: 12),
                  child: OutlinedButton(
                    onPressed: () =>
                        runAction(context, link.act, {'action': 'accept_recommended', 'task_id': taskId}),
                    child: const Text('Aceptar todas las recomendadas'),
                  ),
                ),
              for (final item in items)
                if (item.decision != null || item.approval != null || item.qaProblem != null || item.proposal != null)
                  InboxCard(item: item, act: link.act),
            ]),
          );
        },
      );
}
