{
  description = "A flake for the Rust project";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
  };

  outputs = { self, nixpkgs, ... }:
    let

      system = "x86_64-linux";
      pkgs = import nixpkgs {
        inherit system;
        # overlays = overlays;
      };
      rustPackagess = with pkgs; [
        # build deps
        cmake
        curl
        diffutils
        xz.dev
        zlib.dev
        openssl.dev
        mold
        perl
        pkg-config
        elfutils.dev
        ncurses.dev
        strace
        zstd

        bear # generate compile commands
        gdb

        # bmc deps
        iproute2
        memcached

        # python3 scripts
        (pkgs.python3.withPackages
          (python-pkgs: (with python-pkgs;  [
            # select Python packages here
            tqdm
          ])))

      ];

    in
    {
      devShells."${system}" = {
        default = pkgs.mkShell {
          buildInputs = rustPackagess;
          shellHook = ''
            echo "loading rust env"
          '';
        };
      };
    };
}

