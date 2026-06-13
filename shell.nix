with import <nixpkgs> {};

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
}
