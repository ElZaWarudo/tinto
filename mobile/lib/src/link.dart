// The phone's connection to Tinto through the relay: reconnects with
// backoff, keeps the latest snapshot and sends the allowlisted actions
// (docs/contracts/mobile-link.md §4).

import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:web_socket_channel/web_socket_channel.dart';

import 'crypto.dart';
import 'pairing.dart';
import 'snapshot.dart';

enum LinkState {
  /// No connection to the relay; it retries.
  offline,
  connecting,

  /// On the relay, Tinto has not answered (it is closed or offline).
  waitingDesktop,

  /// Tinto heard this phone; the user has to confirm it there.
  pending,
  paired,

  /// Tinto revoked this phone: it has to pair again.
  revoked,
}

class LinkClient extends ChangeNotifier {
  LinkClient(this.pairing);

  final Pairing pairing;
  LinkState state = LinkState.offline;
  Snapshot? snapshot;
  String? error;

  WebSocketChannel? _channel;
  StreamSubscription<dynamic>? _subscription;
  Session _session = Session();
  Future<void> _inbox = Future.value();
  final _waiting = <String, Completer<String?>>{};
  int _nextId = 0;
  Duration _backoff = const Duration(seconds: 1);
  Timer? _retry;
  bool _closed = false;

  /// Each side pings the other every 30 s (§4); this long without a frame
  /// from Tinto, it left.
  static const _pingEvery = Duration(seconds: 30);
  static const _desktopSilence = Duration(seconds: 75);
  Timer? _ping;
  DateTime _heard = DateTime.now();

  void _set(LinkState next, {String? error}) {
    state = next;
    this.error = error;
    notifyListeners();
  }

  Future<void> start() async {
    if (_closed || state == LinkState.revoked) return;
    _set(LinkState.connecting);
    try {
      final channel = WebSocketChannel.connect(pairing.relay);
      await channel.ready.timeout(const Duration(seconds: 20));
      _channel = channel;
      _session = Session();
      _backoff = const Duration(seconds: 1);
      _subscription = channel.stream.listen(
        // One frame at a time, in order: freshness depends on it.
        (data) => _inbox = _inbox.then((_) => _receive(data)),
        onDone: _lost,
        onError: (_) => _lost(),
      );
      _set(LinkState.waitingDesktop);
      _ping?.cancel();
      _ping = Timer.periodic(_pingEvery, (_) => _tick());
      await _send('hello', {'device': pairing.device});
    } catch (e) {
      _lost(error: 'No se pudo conectar con el relay.');
    }
  }

  void _tick() {
    if (state == LinkState.paired && DateTime.now().difference(_heard) > _desktopSilence) {
      _set(LinkState.waitingDesktop);
    }
    if (_session.knowsPeer) _send('ping', {});
  }

  void _lost({String? error}) {
    _ping?.cancel();
    _subscription?.cancel();
    _subscription = null;
    _channel = null;
    for (final waiting in _waiting.values) {
      waiting.complete('Se cortó la conexión con Tinto.');
    }
    _waiting.clear();
    if (_closed || state == LinkState.revoked) return;
    _set(LinkState.offline, error: error);
    _retry?.cancel();
    _retry = Timer(_backoff, start);
    final doubled = _backoff * 2;
    _backoff = doubled > const Duration(seconds: 60) ? const Duration(seconds: 60) : doubled;
  }

  Future<void> _send(String type, Map<String, dynamic> body) async {
    final channel = _channel;
    if (channel == null) return;
    channel.sink.add(await pairing.key.seal(_session.wrap(type, body)));
  }

  Future<void> _receive(dynamic data) async {
    if (data is! List<int>) return;
    final plain = await pairing.key.open(data);
    final message = plain == null ? null : _session.accept(plain);
    if (message == null) return;
    _heard = DateTime.now();
    switch (message.type) {
      case 'ping':
        if (state == LinkState.waitingDesktop && snapshot != null) _set(LinkState.paired);
      case 'hello':
        final paired = message.body['status'] == 'paired';
        if (message.answerHello) await _send('hello', {'device': pairing.device});
        if (!paired) _set(LinkState.pending);
        if (paired && state != LinkState.paired && snapshot != null) _set(LinkState.paired);
      case 'snapshot':
        snapshot = Snapshot(message.body);
        _set(LinkState.paired);
      case 'ack':
        _waiting.remove(message.body['id'])?.complete(null);
      case 'error':
        final text = message.body['message'];
        _waiting.remove(message.body['id'])?.complete(text is String ? text : 'Tinto no pudo hacerlo.');
      case 'revoked':
        _set(LinkState.revoked);
        await close();
    }
  }

  /// Sends an action; resolves to null once Tinto did it, else why not.
  Future<String?> act(Map<String, dynamic> action) async {
    if (state != LinkState.paired || !_session.knowsPeer) {
      return 'Tinto no está conectado.';
    }
    final id = '${++_nextId}';
    final done = Completer<String?>();
    _waiting[id] = done;
    await _send('action', {'id': id, 'action': action});
    return done.future.timeout(const Duration(seconds: 60), onTimeout: () {
      _waiting.remove(id);
      return 'Tinto no respondió a tiempo.';
    });
  }

  Future<void> close() async {
    _closed = true;
    _retry?.cancel();
    _ping?.cancel();
    await _subscription?.cancel();
    await _channel?.sink.close();
    _channel = null;
  }

  @override
  void dispose() {
    close();
    super.dispose();
  }
}
