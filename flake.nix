# SPDX-License-Identifier: EUPL-1.2
{
  description = "Development shell of static-sql-lineage";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/31ecbaca1bdefa45dffc409f0250bd8039ea0010";
  };

  outputs = { nixpkgs, ... }:
    let
      # No C compiler wrapper: it splits a project path that holds a space.
      # Linking uses the cc on PATH.
      shell = pkgs: pkgs.mkShellNoCC {
        packages = [
          pkgs.cargo pkgs.rustc pkgs.clippy pkgs.rustfmt pkgs.git pkgs.gnumake
          # For the release script and its tests, and for the workflows.
          pkgs.python3
          pkgs.ruff
          pkgs.actionlint
          # For `make size`.
          pkgs.tokei
        ];
      };
    in
    {
      devShells.x86_64-linux.default = shell nixpkgs.legacyPackages.x86_64-linux;
      devShells.aarch64-darwin.default = shell nixpkgs.legacyPackages.aarch64-darwin;
    };
}
