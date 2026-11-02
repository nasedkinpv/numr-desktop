cask "numr-desktop" do
  arch arm: "aarch64", intel: "x64"

  version "0.9.1"
  sha256 arm:   "SKIP_ARM",
         intel: "SKIP_INTEL"

  url "https://github.com/nasedkinpv/numr-desktop/releases/download/v#{version}/numr_#{version}_#{arch}.dmg"
  name "numr"
  desc "Desktop app for the numr text calculator"
  homepage "https://github.com/nasedkinpv/numr-desktop"

  livecheck do
    url :url
    strategy :github_latest
  end

  depends_on macos: ">= :monterey"

  app "numr.app"

  zap trash: [
    "~/Library/Application Support/cc.numr.desktop",
    "~/Library/Caches/cc.numr.desktop",
    "~/Library/WebKit/cc.numr.desktop",
  ]
end
