class Vllora < Formula
  desc "vllora - Multi-provider AI gateway server"
  homepage "https://github.com/vllora/vllora"
  version "0.4.0-prerelease-10"  # Update this with your version
  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/vllora/vllora/releases/download/v0.4.0-prerelease-10/vllora-macos-aarch64"
      sha256 "5e6ed0da4e008d950d5fc66dc12fe5f1d42165fc9b408cefa7cd4964e77de5dc"  # Run: shasum -a 256 vllora-aarch64
    else
      url "https://github.com/vllora/vllora/releases/download/v0.4.0-prerelease-10/vllora-macos-x86_64"
      sha256 "f31497f451e2546e58aa1e6f58ea3abcb22e57fe27014876a10c1cc29ae160a3"  # Run: shasum -a 256 vllora-x86_64
    end
  end
  on_linux do
    if Hardware::CPU.arm?
      url "https://github.com/vllora/vllora/releases/download/v0.4.0-prerelease-10/vllora-linux-aarch64"
      sha256 "placeholder_linux_arm_sha256"  # Will be updated by CI
    else
      url "https://github.com/vllora/vllora/releases/download/v0.4.0-prerelease-10/vllora-linux-x86_64"
      sha256 "63c4bc1600ed1c3fdb600d82cb64e481a9b8d2832aa33f1420902b91fcee790a"  # Run: shasum -a 256 vllora-x86_64
    end
  end
  def install
    if OS.mac?
      if Hardware::CPU.arm?
        bin.install "vllora-macos-aarch64" => "vllora"
      else
        bin.install "vllora-macos-x86_64" => "vllora"
      end
    elsif OS.linux?
      if Hardware::CPU.arm?
        bin.install "vllora-linux-aarch64" => "vllora"
      else
        bin.install "vllora-linux-x86_64" => "vllora"
      end
    end
  end
  def caveats
    # <<~EOS
    #   Server is running on port 8080 and UI can be accessed at http://localhost:8084
    # EOS
  end
  test do
    system "#{bin}/vllora", "--version"
  end
end