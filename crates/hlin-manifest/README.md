# hlin-manifest

The contract between a platform and [Hlin](https://github.com/colliery-io/hlin):
the manifest a platform serves about itself, and the data envelopes its panels
return.

Hlin is a composition shell. Each platform declares, at runtime, what it can
show; the shell renders every declared panel through one design system. Nothing
crosses the boundary except data and a declared view kind. This crate is that
boundary, and it depends on nothing else in the project.

## What is in here

**Envelopes.** Five shapes a panel's data endpoint can return: `scalar.v1`,
`series.v1`, `records.v1`, `status.v1` and `options.v1`, with the limits each is
checked against. Over-limit documents are rejected rather than truncated,
because a silently shortened table is worse than a panel that says it could not
be shown.

**The manifest.** What a platform declares: its panels, the kind each is drawn
as, the envelope each returns, the parameters each responds to, and lifecycle
for anything on its way out.

**Contract identity.** A fingerprint over the parts of a manifest that are a
promise, computed as SHA-256 over RFC 8785 canonical JSON, and a diff that
classifies one contract against another. Titles, descriptions and navigation are
excluded: the hash moves when a promise moves, not when a word changes.

## Features

`contract` is on by default and carries the manifest, the fingerprint and the
diff.

A design pack draws envelopes and never computes a hash, so it takes this crate
without it and stops compiling `sha2`, `semver` and `serde_jcs` for code it will
not call:

```toml
hlin-manifest = { version = "0.0.1", default-features = false }
```

## Licence

MIT.
