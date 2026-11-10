// The link's encryption and freshness (docs/contracts/mobile-link.md §3):
// ChaCha20-Poly1305 with a random nonce per frame, and an envelope that
// names both sessions and a sequence number so recorded frames are refused.

import 'dart:convert';
import 'dart:math';
import 'dart:typed_data';

import 'package:cryptography/cryptography.dart';

const aad = 'tinto-link/1';
const protocolVersion = 1;
const _nonceLength = 12;
const _tagLength = 16;

final _random = Random.secure();

List<int> randomBytes(int length) =>
    List<int>.generate(length, (_) => _random.nextInt(256));

String b64url(List<int> bytes) => base64Url.encode(bytes).replaceAll('=', '');

List<int>? unb64url(String text) {
  try {
    return base64Url.decode(base64Url.normalize(text));
  } on FormatException {
    return null;
  }
}

/// A pairing's key.
class LinkKey {
  LinkKey._(this._key);

  final SecretKey _key;
  static final _aead = Chacha20.poly1305Aead();
  static final _aad = utf8.encode(aad);

  static LinkKey? fromB64(String text) {
    final bytes = unb64url(text);
    if (bytes == null || bytes.length != 32) return null;
    return LinkKey._(SecretKey(bytes));
  }

  /// `nonce || ciphertext || tag`; the nonce is random unless a test gives one.
  Future<Uint8List> seal(List<int> plaintext, {List<int>? nonce}) async {
    final box = await _aead.encrypt(
      plaintext,
      secretKey: _key,
      nonce: nonce ?? randomBytes(_nonceLength),
      aad: _aad,
    );
    return Uint8List.fromList([...box.nonce, ...box.cipherText, ...box.mac.bytes]);
  }

  /// The plaintext, or null for a frame this key did not seal.
  Future<List<int>?> open(List<int> frame) async {
    if (frame.length < _nonceLength + _tagLength) return null;
    final box = SecretBox(
      frame.sublist(_nonceLength, frame.length - _tagLength),
      nonce: frame.sublist(0, _nonceLength),
      mac: Mac(frame.sublist(frame.length - _tagLength)),
    );
    try {
      return await _aead.decrypt(box, secretKey: _key, aad: _aad);
    } on SecretBoxAuthenticationError {
      return null;
    }
  }
}

/// A message that passed the freshness rules.
class Incoming {
  Incoming(this.type, this.body, {required this.answerHello});

  final String type;
  final Map<String, dynamic> body;

  /// A hello from a peer that does not know this session yet.
  final bool answerHello;
}

/// The phone's side of one connection.
class Session {
  Session({this.role = 'phone', String? sid}) : sid = sid ?? b64url(randomBytes(16));

  final String role;
  final String sid;
  int _seq = 0;
  String? _peerSid;
  int _peerSeq = 0;

  /// The other side's earlier sessions: a hello from one is a replay.
  final _retired = <String>{};

  bool get knowsPeer => _peerSid != null;

  List<int> wrap(String type, Map<String, dynamic> body) {
    _seq += 1;
    return utf8.encode(jsonEncode({
      ...body,
      'v': protocolVersion,
      'from': role,
      'type': type,
      'sid': sid,
      'peer': _peerSid ?? '',
      'seq': _seq,
    }));
  }

  /// The message, or null when stale, replayed, reflected or not ours.
  Incoming? accept(List<int> plaintext) {
    final Object? decoded;
    try {
      decoded = jsonDecode(utf8.decode(plaintext));
    } on FormatException {
      return null;
    }
    if (decoded is! Map<String, dynamic>) return null;
    final body = Map<String, dynamic>.of(decoded);
    final from = body['from'], type = body['type'], sid = body['sid'];
    final peer = body['peer'], seq = body['seq'];
    if (body['v'] != protocolVersion ||
        from is! String ||
        from == role ||
        type is! String ||
        sid is! String ||
        sid.isEmpty ||
        peer is! String ||
        seq is! int) {
      return null;
    }
    final fresh = _peerSid != sid || seq > _peerSeq;
    final bool answerHello;
    if (type == 'hello') {
      if (!fresh || _retired.contains(sid)) return null;
      answerHello = peer != this.sid;
    } else {
      if (_peerSid != sid || peer != this.sid || !fresh) return null;
      answerHello = false;
    }
    final old = _peerSid;
    if (old != null && old != sid) _retired.add(old);
    _peerSid = sid;
    _peerSeq = seq;
    for (final field in ['v', 'from', 'type', 'sid', 'peer', 'seq']) {
      body.remove(field);
    }
    return Incoming(type, body, answerHello: answerHello);
  }
}
