# Running Behind Fluxheim

Mythenheim must run cleanly behind Fluxheim as an upstream HTTP service.

Minimal Fluxheim route shape:

```toml
[[vhosts]]
name = "mythenheim"
hosts = ["mythenheim.eu", "dev.mythenheim.eu"]

[vhosts.proxy]
upstreams = ["127.0.0.1:37171"]
upstream_tls = false
```

Mythenheim-side requirements:

- set `server.public_base_url` to `https://mythenheim.eu` in production;
- use `https://dev.mythenheim.eu` for this machine's development deployment;
- configure `server.trusted_proxy_cidrs` for the Fluxheim hop;
- trust forwarded headers only from configured proxy CIDRs;
- keep cookies `Secure` in production because TLS terminates at Fluxheim;
- expose `/healthz` for upstream health checks.

The first commit includes `examples/fluxheim-wolfi-mythenheim.toml` for a
rootless Wolfi container smoke test. Forwarded header enforcement becomes part
of the stable auth/session work.
