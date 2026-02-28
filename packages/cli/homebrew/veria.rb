class Veria < Formula
  desc "Solana-native ZK Coprocessor CLI. Few dots. Whole truth."
  homepage "https://veria.fun"
  url "https://registry.npmjs.org/veria-cli/-/veria-cli-0.1.0.tgz"
  sha256 "REPLACE_WITH_SHA256_AFTER_NPM_PUBLISH"
  license "Apache-2.0"

  depends_on "node@20"

  def install
    system "npm", "install", *Language::Node.std_npm_install_args(libexec)
    bin.install_symlink Dir["#{libexec}/bin/*"]
  end

  test do
    assert_match "0.1.0", shell_output("#{bin}/veria --version")
  end
end
