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
        ninja # rust build
        (hiPrio gcc)
        libgcc
        curl
        diffutils
        xz.dev
        llvm
        clang
        lld
        clang-tools
        zlib.dev
        openssl.dev
        flex
        bison
        busybox
        qemu
        mold
        perl
        pkg-config
        elfutils.dev
        ncurses.dev
        rust-bindgen
        pahole
        strace
        zstd
        eza

        bear # generate compile commands
        rsync # for make headers_install
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

        zoxide # in case host is using zoxide
        openssh # q-script ssh support
      ];

    in
    {
      devShells."${system}" = {
        default = pkgs.mkShell {
          inputsFrom = [ pkgs.linux_latest ];
          buildInputs = rustPackagess;
          hardeningDisable = [ "strictoverflow" "zerocallusedregs" ];

          shellHook = ''
            echo "loading rust env"
          '';
        };
      };
    };
}

