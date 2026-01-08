{
  description = "BlueVault - A TUI application for managing Blu-ray cold storage archives on Linux";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.flake-utils.follows = "flake-utils";
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };

        # Use rust-bin from rust-overlay for latest stable Rust
        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rustfmt" "clippy" ];
        };

        # Build the Rust package
        bluevault = pkgs.rustPlatform.buildRustPackage {
          pname = "bdarchive";
          version = "0.1.0";
          src = ./.;
          
          cargoLock = {
            lockFile = ./Cargo.lock;
          };

          nativeBuildInputs = with pkgs; [
            rustToolchain
            pkg-config
            makeWrapper
          ];

          buildInputs = with pkgs; [
            # System dependencies
            xorriso
            dvdplusrwtools  # provides growisofs
            qrencode
            rsync
            util-linux      # provides mount/umount
            coreutils       # provides sha256sum and other utilities
          ];

          # Set environment variables for runtime dependencies
          postInstall = ''
            # Create a wrapper that ensures dependencies are in PATH
            wrapProgram $out/bin/bdarchive \
              --prefix PATH : ${pkgs.lib.makeBinPath [
                pkgs.xorriso
                pkgs.dvdplusrwtools
                pkgs.qrencode
                pkgs.rsync
                pkgs.util-linux
                pkgs.coreutils
              ]}
          '';

          meta = with pkgs.lib; {
            description = "BlueVault - A TUI application for managing Blu-ray cold storage archives on Linux";
            homepage = "https://github.com/ChrisLAS/blue-vault";
            license = licenses.gpl2Only;
            maintainers = [ ];
            platforms = platforms.linux;
          };
        };

      in
      {
        # The default package (built with `nix build`)
        packages.default = bluevault;

        # The app (built with `nix run`)
        apps.default = flake-utils.lib.mkApp {
          drv = bluevault;
        };

        # Development shell (activated with `nix develop`)
        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            rustToolchain
            xorriso
            dvdplusrwtools
            qrencode
            rsync
            util-linux
            coreutils
            # Development tools
            cargo-edit
            cargo-watch
            cargo-audit
            cargo-outdated
            rust-analyzer
          ];

          shellHook = ''
            echo "BlueVault Development Environment"
            echo "=================================="
            echo "Rust: $(rustc --version)"
            echo "Cargo: $(cargo --version)"
            echo ""
            echo "Available tools:"
            echo "  - xorriso: $(xorriso --version 2>&1 | head -1)"
            echo "  - growisofs: $(growisofs --version 2>&1 | head -1)"
            echo "  - qrencode: $(qrencode -V 2>&1 | head -1)"
            echo "  - rsync: $(rsync --version | head -1)"
            echo ""
            echo "Run 'cargo build' to build the project"
            echo "Run 'cargo run' to run the application"
            echo "Run 'cargo test' to run tests"
          '';

          # Ensure all tools are in PATH
          nativeBuildInputs = with pkgs; [
            rustToolchain
            xorriso
            dvdplusrwtools
            qrencode
            rsync
            util-linux
            coreutils
          ];
        };
      });
}

