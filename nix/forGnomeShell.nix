# Returns a function `shell -> extensions -> [derivation]`.
#
# Usage:
#   pkgs.nix4gnome.forGnomeShell "46" [
#     "blur-my-shell@aunetx"          # latest version compatible with shell 46
#     "caffeine@patapon.info.60"      # pinned extension version
#   ]
{
  pkgs,
  dataPath ? ../data/gnome,
}:
let
  lib = pkgs.lib;
in
shell: extensions:
let
  allExtensions =
    let
      files = builtins.attrNames (
        lib.filterAttrs (name: type: type == "regular" && lib.hasSuffix ".json" name) (
          builtins.readDir dataPath
        )
      );
    in
    lib.foldl' (
      acc: file: acc // builtins.fromJSON (builtins.readFile (dataPath + "/${file}"))
    ) { } files;

  normalize =
    entry:
    if lib.isString entry then
      {
        uuid = entry;
        version = null;
      }
    else
      {
        uuid = entry.uuid;
        version = entry.version or null;
      };

  lookup =
    {
      uuid,
      version,
    }:
    let
      all =
        allExtensions.${uuid} or (throw ''
          Extension `${uuid}` not found in data.
          The dataset may be partial (CI populates it incrementally) or the uuid may be misspelled.
        '');
      matching = builtins.filter (
        entry:
        builtins.elem shell entry.s
        && (version == null || builtins.toString entry.v == builtins.toString version)
      ) all;
      # NB: no throw as foldl' seed — foldl' forces the accumulator before
      # the first step and would trip the throw on non-empty candidate lists.
      pickBest = builtins.foldl' (
        acc: entry:
        if builtins.compareVersions (builtins.toString entry.v) (builtins.toString acc.v) > 0 then
          entry
        else
          acc
      ) (builtins.head matching) (builtins.tail matching);
    in
    if builtins.length matching == 0 then
      throw ''
        No version of `${uuid}` compatible with GNOME Shell ${shell}.
        Available shells: ${lib.concatStringsSep ", " (lib.concatMap (entry: entry.s) all)}
      ''
    else
      pickBest;

  mkExtension =
    uuid: entry:
    pkgs.stdenv.mkDerivation {
      pname = lib.head (lib.splitString "@" uuid);
      version = builtins.toString entry.v;

      src = pkgs.fetchzip {
        url = "https://extensions.gnome.org/download-extension/${uuid}.shell-extension.zip?version_tag=${builtins.toString entry.t}";
        sha256 = entry.h;
        # Most extension zips carry metadata.json at their root; stripRoot
        # would otherwise fail or misbehave.
        stripRoot = false;
      };

      dontConfigure = true;
      dontBuild = true;

      installPhase = ''
        runHook preInstall
        src_root="$src"
        if [ ! -f "$src_root/metadata.json" ]; then
          sub="$(find "$src_root" -mindepth 1 -maxdepth 1 -type d | head -n1)"
          if [ -f "$sub/metadata.json" ]; then
            src_root="$sub"
          fi
        fi
        mkdir -p "$out/share/gnome-shell/extensions/${uuid}"
        cp -r "$src_root/." "$out/share/gnome-shell/extensions/${uuid}/"
        runHook postInstall
      '';

      passthru = {
        extensionUuid = uuid;
        extensionVersion = entry.v;
        gnomeShellVersions = entry.s;
      };

      meta = {
        description = "GNOME Shell extension ${uuid}";
        platforms = lib.platforms.linux;
      };
    };
in
map (entry: mkExtension (normalize entry).uuid (lookup (normalize entry))) extensions
