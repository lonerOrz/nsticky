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

    menu = mkOption {
      type = with types; nullOr str;
      default = null;
      example = literalExpression ''"pantry -m"'';
      description = ''
        Command used by `stage restore` to pick staged window(s). Any
        dmenu-compatible program works. Overrides the `menu` key in
        `settings`. When null, falls back to the built-in terminal prompt.
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

    xdg.configFile."nsticky/config.toml" = mkIf (cfg.settings != { } || cfg.menu != null) {
      source = tomlFormat.generate "sticky-config" (cfg.settings // lib.optionalAttrs (cfg.menu != null) { menu = cfg.menu; });
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
