# nix-gnome-extension

GNOME Shell extensions packaged for Nix, powered by live data from
[extensions.gnome.org](https://extensions.gnome.org).

Rust exporter pulls extension metadata + asset hashes; a Nix library turns
them into derivations. Modeled after
[nix4vscode](https://github.com/nix-community/nix4vscode).

## Usage

```nix
{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    nix-gnome-extension.url = "github:cathaysia/nix-gnome-extension";
  };

  outputs = { nixpkgs, nix-gnome-extension, ... }:
    let
      pkgs = import nixpkgs {
        system = "x86_64-linux";
        overlays = [ nix-gnome-extension.overlays.default ];
      };
    in {
      # Latest version of each extension compatible with GNOME Shell 46
      extensions = pkgs.nix4gnome.forGnomeShell "46" [
        "blur-my-shell@aunetx"
        "caffeine@patapon.info"
        # Pin an extension version
        "dash-to-dock@micxgx.gmail.com.105"
      ];
    };
}
```

Each result is a derivation installing the unpacked extension under
`$out/share/gnome-shell/extensions/<uuid>/`, with `passthru.extensionUuid` /
`extensionVersion` / `gnomeShellVersions`.

## Exporter CLI

```console
$ exporter --help
--fetch              enumerate all extensions into the state file
--hash               compute missing asset hashes (concurrent, resumable)
--output <dir>       write sharded JSON for the Nix library
--state <path>       state file (default: state.json)
--batch-size <n>     hash concurrency (default: 8)
--max-pages <n>      cap enumeration pages (smoke tests)
--limit <n>          cap hashed assets per run (smoke tests)
--max-run-time <s>   abort hashing after N seconds, keep progress
```

Typical full run (what CI does daily):

```bash
exporter --fetch
exporter --hash --batch-size 8 --max-run-time 18000
exporter --output data/gnome
```

State (`state.json`) is resumable and persisted on the `db` branch by CI;
data shards land in `data/gnome/data_*.json` on master. The dataset grows
incrementally across CI runs until coverage is complete.

## API notes (verified 2026-08)

- Enumeration: `GET /extension-query/?sort=<name|downloads|recent>&page=<n>`
  — fixed page size 10 (~325 pages), items carry
  `shell_version_map: {"46": {pk, version}}`.
- Asset: `GET /download-extension/<uuid>.shell-extension.zip?version_tag=<pk>`
  — 302 to `/api/v1/extensions/<uuid>/versions/<v>/?format=zip`.
- A REST surface exists under `/api/v1/` (`count: 5154`) but its versions
  endpoint lacks download URLs; enumeration therefore uses `/extension-query/`,
  which yields only extensions that are actually installable.
- Hashes are SHA-256 in Nix base32, byte-identical to `nix-prefetch-url`.

## Development

```bash
cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test
nix flake check   # builds the exporter package
```
