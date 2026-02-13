{
  outputs = inputs@{
    self, nixpkgs, flake-parts,
  }: let
    sactor = { rustPlatform }: rustPlatform.buildRustPackage {
      name = "sactor";
      src = ./.;
      cargoLock.lockFile = ./Cargo.lock;
    };
  in flake-parts.lib.mkFlake { inherit inputs; } {
    systems = [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];
    perSystem = { self', pkgs, ... }: {
      packages.default = pkgs.callPackage sactor {};
      devShells.default = pkgs.mkShell {
        inputsFrom = [ self'.packages.default ];
        buildInputs = with pkgs; [];
        nativeBuildInputs = with pkgs; [ clippy ];
      };
    };
  };
}
