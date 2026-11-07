# Tinto Delivery (móvil)

Sigue los lotes de Delivery y responde decisiones, aprobaciones y problemas de
QA desde el móvil. Habla con Tinto a través de un relay que solo reenvía
mensajes cifrados de extremo a extremo; el contrato está en
[`docs/contracts/mobile-link.md`](../docs/contracts/mobile-link.md).

- `lib/src/crypto.dart`: ChaCha20-Poly1305 y las reglas de frescura (Dart puro).
- `lib/src/pairing.dart`: el enlace del QR y su guardado seguro.
- `lib/src/link.dart`: la conexión con el relay, reconexión y acciones.
- `lib/src/snapshot.dart`, `widgets.dart`, `screens.dart`: lo que se muestra.

Flutter 3.16 / Dart 3.2:

```sh
flutter pub get
flutter analyze
flutter test
```
