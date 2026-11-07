// A pairing from Tinto's QR (docs/contracts/mobile-link.md §1), and where
// the phone keeps it: the platform's secure storage.

import 'dart:convert';

import 'package:flutter_secure_storage/flutter_secure_storage.dart';

import 'crypto.dart';

final _room = RegExp(r'^[A-Za-z0-9_-]{22}$');

class Pairing {
  Pairing._(this.link, this.relay, this.room, this.key, this.device);

  /// What the QR held, key included.
  final String link;

  /// The room's address on the relay, for the phone.
  final Uri relay;
  final String room;
  final LinkKey key;

  /// The name Tinto shows for this phone.
  final String device;

  /// `tinto-link://host[:port]/<room>[?tls=0]#<key>`, or null when it is not one.
  static Pairing? parse(String link, {String device = 'Móvil'}) {
    final Uri uri;
    try {
      uri = Uri.parse(link.trim());
    } on FormatException {
      return null;
    }
    if (uri.scheme != 'tinto-link' || uri.host.isEmpty || uri.pathSegments.length != 1) {
      return null;
    }
    final room = uri.pathSegments.single;
    final key = LinkKey.fromB64(uri.fragment);
    if (!_room.hasMatch(room) || key == null) return null;
    final relay = Uri(
      scheme: uri.queryParameters['tls'] == '0' ? 'ws' : 'wss',
      host: uri.host,
      port: uri.hasPort ? uri.port : null,
      path: '/v1/rooms/$room',
      queryParameters: {'role': 'phone'},
    );
    final name = device.trim().isEmpty ? 'Móvil' : device.trim();
    return Pairing._(link.trim(), relay, room, key, name.length > 64 ? name.substring(0, 64) : name);
  }
}

class PairingStore {
  PairingStore([FlutterSecureStorage? storage]) : _storage = storage ?? const FlutterSecureStorage();

  final FlutterSecureStorage _storage;
  static const _slot = 'tinto_pairing';

  Future<Pairing?> load() async {
    final saved = await _storage.read(key: _slot);
    if (saved == null) return null;
    final fields = jsonDecode(saved) as Map<String, dynamic>;
    return Pairing.parse(fields['link'] as String, device: fields['device'] as String? ?? '');
  }

  Future<void> save(Pairing pairing) =>
      _storage.write(key: _slot, value: jsonEncode({'link': pairing.link, 'device': pairing.device}));

  Future<void> forget() => _storage.delete(key: _slot);
}
