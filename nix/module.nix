{ lib, pkgs, config, ... }:

let
  inherit (lib)
  mkOption
  mkEnableOption
  types
  mkIf
  literalExpression
  ;

  cfg = config.programs.nsticky;
  tomlFormat = pkgs.formats.toml { };
in
{
  options.programs.nsticky = {
    enable = mkEnableOption "nsticky";
    package = mkOption {
      type = with types; nullOr package;
      default = pkgs.callPackage ./package.nix { };
      description = ''
        The nsticky package to use.
      '';
    };
    
    settings = mkOption {
      inherit (tomlFormat) type;
      default = { };
      example = literalExpression ''
        {
          sticky = {
          firefox.app-id = "firefox";

          kitty = {
            app-id = "kitty";
            title = ".*server.*";
          };

          gmail.title = ".*Gmail.*";
        }
      '';
      description = ''
        Configuration written to
        {file}`$XDG_CONFIG_HOME/nsticky/config.toml`.
      '';
    };
  };

  config = mkIf cfg.enable {
    home.packages = mkIf (cfg.package != null ) [
      cfg.package
    ];

    xdg.configFile."nsticky/config.toml" = mkIf (cfg.settings != { }) {
      source = tomlFormat.generate "sticky-config" cfg.settings;
    };

    systemd.user.services.nsticky = {
      Unit = {
        Description = "nsticky service";
        PartOf = [ config.wayland.systemd.target ];
        After = [ config.wayland.systemd.target ];
      };

      Service = {
        Type = "simple";
        ExecStart = "${lib.getExe cfg.package}";
        Restart = "on-failure";
      };

      Install.WantedBy = [ config.wayland.systemd.target ];
    };
  };
}
