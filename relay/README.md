# tinto-relay

The blind relay between Tinto and its phone app. It forwards end-to-end
encrypted frames between the two peers of a room and keeps nothing but the
live sockets: no accounts, no storage, no logs of contents. The protocol is
in [`docs/contracts/mobile-link.md`](../docs/contracts/mobile-link.md).

- `GET /healthz` → `200 ok`
- `wss://<host>/v1/rooms/<roomId>?role=desktop|phone`
- Frames ≤ 256 KiB, binary only; ≤ 60 frames and 4 MiB per 10 s per socket.
- At most 2 000 sockets (and so rooms) at once; more get HTTP 503.

## Run

```sh
npm ci
npm start            # PORT=8787 by default
npm test             # node --test
```

## Deploy behind TLS

The relay speaks plain `ws://`; put it behind a reverse proxy that ends TLS,
so Tinto and the phone connect with `wss://`. The contents are encrypted
either way, but TLS hides room ids from the network.

```sh
docker build -t tinto-relay .
docker run -d --restart=unless-stopped -p 127.0.0.1:8787:8787 tinto-relay
```

Caddy (gets and renews the certificate itself):

```
relay.example.com {
    reverse_proxy 127.0.0.1:8787
}
```

nginx:

```nginx
location / {
    proxy_pass http://127.0.0.1:8787;
    proxy_http_version 1.1;
    proxy_set_header Upgrade $http_upgrade;
    proxy_set_header Connection "upgrade";
    proxy_read_timeout 120s;
}
```

Then set `wss://relay.example.com` as the relay in Tinto
(Ajustes → Delivery → Móvil).

Keep proxy access logs off or short-lived: they hold room ids and IPs, which
is all the relay can know.
