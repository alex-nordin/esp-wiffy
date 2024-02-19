{
<<<<<<< HEAD
  description = "Development shell for esp32 board";

=======
  description = "Development shell for esp32-c3 board";
>>>>>>> 69ce78ab56d4a12475a2091d8a00381c72a52fce
  inputs = {
    flake-parts.url = "github:hercules-ci/flake-parts";
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    # agenix-shell.url = "github:aciceri/agenix-shell";
    devenv.url = "github:cachix/devenv";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs = inputs @ {flake-parts, ...}:
    flake-parts.lib.mkFlake {inherit inputs;} {
      imports = [
        # inputs.agenix-shell.flakeModules.default
        inputs.devenv.flakeModule
        # To import a flake module
        # 1. Add foo to inputs
        # 2. Add foo as a parameter to the outputs function
        # 3. Add here: foo.flakeModule
      ];
      systems = ["x86_64-linux"];
      # agenix-shell = {
      #   secrets = {
      #     wifi_p.file = ./secrets/wifi_p.age;
      #   };
      # };
      perSystem = {
        config,
        self',
        inputs',
        pkgs,
        system,
        lib,
        ...
      }: {
        # Per-system attributes can be defined here. The self' and inputs'
        # module parameters provide easy access to attributes of the same
        # system.
        _module.args.pkgs = import inputs.nixpkgs {
          inherit system;
          overlays = [
            inputs.rust-overlay.overlays.default
          ];
        };

        # Equivalent to  inputs'.nixpkgs.legacyPackages.hello;
        # packages.default = pkgs.hello;
        devenv.shells.default = {
          packages = with pkgs; [
            (rust-bin.selectLatestNightlyWith (toolchain:
              toolchain.default.override {
                extensions = ["rust-src"];
                targets = ["riscv32imc-unknown-none-elf"];
              }))
            rage
            openssl
            pkg-config
            cargo-generate
          ];

          enterShell = ''
          WIFI=$(rage -d ./secrets/wiffy.age -i ~/.config/sops/age/keys.txt)
          echo $WIFI
          '';
        };
      };
<<<<<<< HEAD
    };
=======
    in 
    with pkgs;
    {
      devShells.default = mkShell {
        nativeBuildInputs = [
        openssl
        pkg-config
        cargo-generate
        rage
        (rust-bin.selectLatestNightlyWith (toolchain: toolchain.default.override {
          extensions = [ "rust-src" ];
          targets = ["riscv32imc-unknown-none-elf"];
        }))
        ];
      shellHook = ''
      WIFI=$(rage -d ./secrets/secret.txt.age -i ~/.config/sops/age/keys.txt)
      echo $WIFI
      '';
      };
    });
>>>>>>> 69ce78ab56d4a12475a2091d8a00381c72a52fce
}

