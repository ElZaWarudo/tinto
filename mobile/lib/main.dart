// Tinto Delivery on the phone: follow and answer Delivery batches through a
// blind relay, end-to-end encrypted (docs/contracts/mobile-link.md).

import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';

import 'src/link.dart';
import 'src/pairing.dart';
import 'src/screens.dart';

void main() => runApp(const TintoApp());

class TintoApp extends StatefulWidget {
  const TintoApp({super.key});

  @override
  State<TintoApp> createState() => _TintoAppState();
}

class _TintoAppState extends State<TintoApp> {
  final _store = PairingStore();
  LinkClient? _link;
  bool _loading = true;

  @override
  void initState() {
    super.initState();
    _store.load().then((pairing) {
      if (!mounted) return;
      setState(() {
        _loading = false;
        if (pairing != null) _connect(pairing);
      });
    });
  }

  void _connect(Pairing pairing) => _link = LinkClient(pairing)..start();

  Future<void> _paired(Pairing pairing) async {
    await _store.save(pairing);
    setState(() => _connect(pairing));
  }

  Future<void> _unpair() async {
    final link = _link;
    setState(() => _link = null);
    link?.dispose();
    await _store.forget();
  }

  @override
  Widget build(BuildContext context) {
    final link = _link;
    return MaterialApp(
      title: 'Tinto Delivery',
      locale: const Locale('es'),
      supportedLocales: const [Locale('es')],
      localizationsDelegates: GlobalMaterialLocalizations.delegates,
      theme: ThemeData(colorSchemeSeed: const Color(0xFF7A1F3D), useMaterial3: true),
      darkTheme: ThemeData(
        colorSchemeSeed: const Color(0xFF7A1F3D),
        brightness: Brightness.dark,
        useMaterial3: true,
      ),
      home: _loading
          ? const Scaffold(body: Center(child: CircularProgressIndicator()))
          : link == null
              ? PairScreen(onPaired: _paired)
              : HomeScreen(link: link, onUnpair: _unpair),
    );
  }
}
