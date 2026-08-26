{
  description = "Nix overlay packaging GNOME Shell extensions from extensions.gnome.org";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    systems.url = "github:nix-systems/default";
  };

  outputs =
    {
      self,
      nixpkgs,
      systems,
    }:
    let
      eachSystem = f: nixpkgs.lib.genAttrs (import systems) (system: f nixpkgs.legacyPackages.${system});
    in
    {
      lib = eachSystem (pkgs: {
        forGnomeShell = import ./nix/forGnomeShell.nix { inherit pkgs; };
      });

      overlays.default = final: _: {
        nix4gnome = self.lib.${final.stdenv.hostPlatform.system};
      };

      packages = eachSystem (
        pkgs:
        let
          exporter = pkgs.rustPlatform.buildRustPackage {
            pname = "nix-gnome-extension";
            version = "0.1.0";
            src = self;
            cargoLock.lockFile = ./Cargo.lock;
            meta.mainProgram = "exporter";
          };
        in
        {
          inherit exporter;
          default = exporter;
        }
      );

      devShells = eachSystem (pkgs: {
        default = pkgs.mkShell {
          strictDeps = true;
          packages = with pkgs; [
            cargo
            clippy
            rustfmt
            nixfmt-rfc-style
          ];
        };
      });

      checks = eachSystem (pkgs: {
        exporter-build = self.packages.${pkgs.system}.exporter;
      });
    };
}
