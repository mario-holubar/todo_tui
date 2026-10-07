{
    description = "Project development environment (migrated from shell.nix)";

    inputs = {
        nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
        flake-utils.url = "github:numtide/flake-utils";
    };

    outputs = { nixpkgs, flake-utils, ... }:
        flake-utils.lib.eachDefaultSystem (system:
            let
                pkgs = nixpkgs.legacyPackages.${system};
            in {
                devShells = {
                    default = with pkgs;

        let dependencies = [
            #xorg.libX11
            #xorg.libXcursor
            #xorg.libXrandr
            #xorg.libXi
            #wayland
            #libGL
            #vulkan-loader
            #libxkbcommon
        ]; in
        mkShell {
            # Tools and stuff
            packages = [
            ];

            # Runtime programs / libraries
            buildInputs = dependencies;

            # Compile-time programs / libraries
            nativeBuildInputs = [
                rustc
                cargo
                rustfmt
                clippy
                rust-analyzer
            ];

            # Environment variables
            env = {
                LD_LIBRARY_PATH = lib.makeLibraryPath dependencies;
                RUST_BACKTRACE = 1;
            };

            # NOTE Does not get run by direnv bc of caching! Put it in .envrc.
            shellHook = "";
        };
                    };
            });
}
