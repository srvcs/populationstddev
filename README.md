# srvcs-populationstddev

The population standard-deviation orchestrator of the srvcs.cloud distributed
standard library.

Its single concern: **statistics: population standard deviation.** It owns the
*control flow* — composing two sibling primitives — but does no arithmetic of
its own. It asks
[`srvcs-populationvariance`](https://github.com/srvcs/populationvariance) for the
variance of the values, then asks [`srvcs-sqrt`](https://github.com/srvcs/sqrt)
for its square root.

```
populationstddev(values):
    v = populationvariance(values)   # the population variance
    return sqrt(v)                    # its square root
```

The result is an `f64` — a JSON number that may be fractional. For example
`populationstddev([1, 2, 3, 4, 5]) ~= 1.4142135623730951` (variance `2.0`, then
`sqrt(2)`).

Validation is not handled here. This service never calls `srvcs-isnumber`
directly; instead its dependencies validate their own operands, and any `422`
they raise (for instance an empty list, or a non-numeric element rejected by
`srvcs-populationvariance`) is forwarded verbatim.

## API

| Method | Path | Purpose |
| --- | --- | --- |
| `GET` | `/` | Service identity, concern, and dependency list |
| `POST` | `/` | Compute the population standard deviation of `values` |
| `GET` | `/healthz` `/readyz` `/metrics` `/openapi.json` | srvcs service standard surface |

```sh
curl -s -X POST localhost:8080/ -H 'content-type: application/json' -d '{"values": [1, 2, 3, 4, 5]}'
# {"values":[1,2,3,4,5],"result":1.4142135623730951}
```

Responses:

- `200 {"values": [...], "result": n}` — evaluated; `result` is a float.
- `422` — a dependency rejected the input (forwarded verbatim), e.g. an empty
  list or a non-numeric element.
- `500` — a reachable dependency returned a `200` without a numeric `result`
  (a contract violation).
- `503` — a dependency is unavailable.

## Dependencies

- [`srvcs-populationvariance`](https://github.com/srvcs/populationvariance)
- [`srvcs-sqrt`](https://github.com/srvcs/sqrt)

## Configuration

| Variable | Default | Purpose |
| --- | --- | --- |
| `SRVCS_BIND_ADDR` | `0.0.0.0:8080` | Bind address |
| `SRVCS_POPULATIONVARIANCE_URL` | `http://127.0.0.1:8090` | Base URL of `srvcs-populationvariance` |
| `SRVCS_SQRT_URL` | `http://127.0.0.1:8091` | Base URL of `srvcs-sqrt` |
| `SRVCS_ENV` | `development` | Environment label for logs |
| `RUST_LOG` | `info,tower_http=info` | Tracing filter |

## Local checks

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Orchestration tests stand up *computing* mock dependency services in-process —
they read the request body and return the real population variance / square
root, so the composition is genuinely exercised against the asserted cases
(compared approximately, since the result is a float). See
[`srvcs/platform`](https://github.com/srvcs/platform) for the shared standard.

> Note: the `cargoHash` in `flake.nix` is inherited from the template and must be
> refreshed with a `nix build` before the Nix gates pass.
