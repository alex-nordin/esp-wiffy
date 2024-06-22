{
  description = "Development shell for esp32 board";
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
            export PASSWORD=$(rage -d ./secrets/wiffy.age -i ~/.config/sops/age/keys.txt)
            export SSID=$(rage -d ./secrets/ssid.age -i ~/.config/sops/age/keys.txt)
            export MQTT_PASS=$(rage -d ./secrets/mqtt_pass.age -i ~/.config/sops/age/keys.txt)
            export MQTT_USER=$(rage -d ./secrets/mqtt_user.age -i ~/.config/sops/age/keys.txt)
            echo $PASSWORD
            echo $SSID
            echo $MQTT_PASS
            echo $MQTT_USER
          '';
        };
      };
    };
}
