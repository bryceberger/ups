{
  inputs = {
    nixpkgs.url = "nixpkgs";
    fenix = {
      url = "github:nix-community/fenix/monthly";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    naersk = {
      url = "github:nix-community/naersk";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = {
    self,
    nixpkgs,
    fenix,
    naersk,
    flake-utils,
  }:
    flake-utils.lib.eachDefaultSystem (system: let
      pkgs = nixpkgs.legacyPackages.${system};
      toolchain = fenix.packages.${system}.complete.toolchain;
      naersk' = (pkgs.callPackage naersk {}).override {
        cargo = toolchain;
        rustc = toolchain;
      };

      ups-apply = naersk'.buildPackage {src = ./.;};
    in {
      packages = {
        inherit ups-apply;
        default = ups-apply;
      };
      devShell = pkgs.mkShell {
        packages = [toolchain];
      };
    });
}
