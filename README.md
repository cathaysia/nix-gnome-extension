# nix-gnome-extension

GNOME Shell extensions packaged for Nix, powered by live data from
[extensions.gnome.org](https://extensions.gnome.org). Modeled after
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

Each result installs the unpacked extension under
`$out/share/gnome-shell/extensions/<uuid>/`, with `passthru.extensionUuid` /
`extensionVersion` / `gnomeShellVersions`.
