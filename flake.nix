{
  description = "tmux-layout development shell";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      nixpkgs,
      flake-utils,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        runtimeDeps = with pkgs; [
          tmux
          yq-go
          gettext
        ];
      in
      {
        packages.default = pkgs.stdenv.mkDerivation {
          pname = "tmux-layout";
          version = pkgs.lib.removeSuffix "\n" (builtins.readFile ./version.txt);
          src = ./.;

          nativeBuildInputs = [ pkgs.makeWrapper ];

          installPhase = ''
            mkdir -p $out/share/tmux-layout $out/bin
            cp tmux-layout version.txt $out/share/tmux-layout/
            cp -r scripts $out/share/tmux-layout/
            chmod +x $out/share/tmux-layout/tmux-layout
            makeWrapper $out/share/tmux-layout/tmux-layout $out/bin/tmux-layout \
              --prefix PATH : ${pkgs.lib.makeBinPath runtimeDeps}
          '';

          meta = with pkgs.lib; {
            description = "Apply YAML-defined tmux layouts";
            homepage = "https://github.com/tmux-contrib/tmux-layout";
            license = licenses.mit;
            maintainers = [ ];
            mainProgram = "tmux-layout";
            platforms = platforms.unix;
          };
        };

        devShells.default = pkgs.mkShell {
          name = "tmux-layout";
          packages = with pkgs; [
            bash
            tmux
            yq-go
            gettext
            bats
            shellcheck
          ];
        };
      }
    );
}
