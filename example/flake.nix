{
  description = "Example: GNOME Shell extensions installed via nix-gnome-extension";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    systems.url = "github:nix-systems/default";
    # Local path while inside this repository; replace with
    # url = "github:cathaysia/nix-gnome-extension"; when copying out.
    nix-gnome-extension = {
      url = "../";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.systems.follows = "systems";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      systems,
      nix-gnome-extension,
    }:
    let
      eachSystem =
        f:
        nixpkgs.lib.genAttrs (import systems) (
          system:
          let
            pkgs = import nixpkgs {
              inherit system;
              overlays = [ nix-gnome-extension.overlays.default ];
            };
          in
          f system pkgs
        );
    in
    {
      packages = eachSystem (
        system: pkgs: rec {
          example-extensions = pkgs.symlinkJoin {
            name = "example-gnome-shell-extensions";
            paths = pkgs.nix4gnome.forGnomeShell "46" [
              "blur-my-shell@aunetx"
              "caffeine@patapon.info"
              # Pin an extension version
              "dash-to-dock@micxgx.gmail.com.105"
            ];
          };
          default = example-extensions;
        }
      );
    };
}
