{
  description = "Various web scrapers";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    utils.url = "github:numtide/flake-utils";
  };

  outputs = {
    self,
    nixpkgs,
    utils,
    ...
  }:
    utils.lib.eachDefaultSystem (system: let
      pkgs = nixpkgs.legacyPackages.${system};
    in {
      packages = {
        default = self.packages.${system}.scrapers;
        scrapers = pkgs.rustPlatform.buildRustPackage {
          name = "scrapers";

          src = pkgs.lib.cleanSource ./.;

          cargoLock = {
            lockFile = ./Cargo.lock;
          };

          nativeBuildInputs = with pkgs; [
            pkg-config
            # protobuf
          ];
          buildInputs = with pkgs; [
            openssl
            # zlib
            # libgit2
          ];
        };
      };
    })
    // {
      nixosModules = {
        default = self.nixosModules.scrapers;
        scrapers = {
          config,
          lib,
          pkgs,
          ...
        }: let
          cfg = config.services.scrapers;
        in {
          options.services.scrapers = {
            enable =
              lib.mkEnableOption "enable scrapers";

            onCalendar = lib.mkOption {
              type = lib.types.str;
              default = "*-*-* 00/8:00:00";
              description = "How often are scrapers started. Default is '*-*-* 00/8:00:00' meaning every 8 hours. See systemd.time(7) for more information about the format.";
            };

            dest = lib.mkOption {
              type = lib.types.str;
            };

            user = lib.mkOption {
              type = lib.types.str;
              default = "scrapers";
            };

            group = lib.mkOption {
              type = lib.types.str;
              default = "scrapers";
            };

            configFile = lib.mkOption {
              type = lib.types.path;
            };
          };
          config = lib.mkIf cfg.enable {
            systemd.tmpfiles.rules = [
              "d '${cfg.dest}' 0750 ${cfg.user} ${cfg.group} - -"
            ];

            users.users = lib.mkIf (cfg.user == "scrapers") {
              scrapers = {
                group = cfg.group;
                home = cfg.dest;
                isSystemUser = true;
                # uid = config.ids.uids.scrapers;
              };
            };

            users.groups = lib.mkIf (cfg.group == "scrapers") {
              # scrapers.gid = config.ids.gids.scrapers;
              # isSystemUser = true;
            };
            systemd.services.scrapers = {
              wantedBy = ["multi-user.target"];
              #unitConfig.ConditionPathExists = "/var/lib/mara-bot/config.yaml";
              serviceConfig = {
                User = cfg.user;
                Group = cfg.group;
                WorkingDirectory = cfg.dest;
                ExecStart = "${self.packages.x86_64-linux.scrapers}/bin/scrapers ${cfg.configFile}";
              };
            };
            systemd.timers.scrapers = lib.mkIf (cfg.onCalendar != null) {
              wantedBy = ["timers.target"];
              partOf = ["scrapers.service"];
              timerConfig = {
                OnCalendar = cfg.onCalendar;
                RandomizedDelaySec = "1 hour";
                Unit = "scrapers.service";
              };
            };
          };
        };
      };
    };
}
