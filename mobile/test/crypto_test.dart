import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:tinto_delivery/src/crypto.dart';
import 'package:tinto_delivery/src/pairing.dart';

// docs/contracts/mobile-link.md §7, computed with Node's OpenSSL; the Rust
// side checks the same vector.
const vectorKey = 'AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8';
const vectorPlaintext =
    '{"v":1,"from":"phone","type":"hello","sid":"c2lkLXBob25lLTAwMDAwMQ","peer":"","seq":1,"device":"Prueba"}';
const vectorFrame =
    '404142434445464748494a4b83760aa3487bc2233ebd996d99d54fbddca30d7d66d0631b877cb779d005520f9cdff6520a69119f6e4d1565096682f5ad0b2ea69c44b84397964e4c452f721686610b717163824e302c57abe1020719b82ab214cd91c20ad807c54e697eb856ce4afb3f4ee5370a59138dded052b70cb1ac0871a1bda69c';

String hex(List<int> bytes) => bytes.map((b) => b.toRadixString(16).padLeft(2, '0')).join();

void main() {
  test('the shared vector seals and opens as the contract says', () async {
    final key = LinkKey.fromB64(vectorKey)!;
    final nonce = List<int>.generate(12, (i) => 0x40 + i);
    final frame = await key.seal(utf8.encode(vectorPlaintext), nonce: nonce);
    expect(hex(frame), vectorFrame);
    expect(utf8.decode((await key.open(frame))!), vectorPlaintext);
  });

  test('a tampered frame or another key does not open', () async {
    final key = LinkKey.fromB64(b64url(randomBytes(32)))!;
    final frame = await key.seal(utf8.encode('hola'));
    final tampered = [...frame]..last ^= 1;
    expect(await key.open(tampered), isNull);
    expect(await LinkKey.fromB64(b64url(randomBytes(32)))!.open(frame), isNull);
  });

  test('sessions refuse replays, reflections and frames of an earlier session', () {
    final desktop = Session(role: 'desktop');
    final phone = Session();
    final hello = desktop.accept(phone.wrap('hello', {'device': 'Pixel'}))!;
    expect(hello.answerHello, isTrue);
    expect(phone.accept(desktop.wrap('hello', {}))!.answerHello, isFalse);

    final snapshot = desktop.wrap('snapshot', {'runs': []});
    expect(phone.accept(snapshot)!.type, 'snapshot');
    expect(phone.accept(snapshot), isNull); // replayed
    expect(phone.accept(phone.wrap('hello', {})), isNull); // reflected

    final restarted = Session(role: 'desktop');
    phone.accept(restarted.wrap('hello', {}));
    expect(phone.accept(desktop.wrap('snapshot', {})), isNull); // old session
    // The relay replays the old session's hello, then what followed it.
    expect(phone.accept(desktop.wrap('hello', {})), isNull);
    expect(phone.accept(desktop.wrap('revoked', {})), isNull);
  });

  test('a pairing link gives the room, the relay and the key', () {
    const room = 'AAAAAAAAAAAAAAAAAAAAAA';
    final secure = Pairing.parse('tinto-link://relay.example.com/$room#$vectorKey', device: 'Ana')!;
    expect(secure.relay.toString(), 'wss://relay.example.com/v1/rooms/$room?role=phone');
    expect(secure.device, 'Ana');
    final local = Pairing.parse('tinto-link://192.168.1.5:8787/$room?tls=0#$vectorKey')!;
    expect(local.relay.toString(), 'ws://192.168.1.5:8787/v1/rooms/$room?role=phone');
    for (final bad in [
      'https://relay.example.com/$room#$vectorKey',
      'tinto-link://relay.example.com/short#$vectorKey',
      'tinto-link://relay.example.com/$room#short',
    ]) {
      expect(Pairing.parse(bad), isNull, reason: bad);
    }
  });
}
