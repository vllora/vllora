class Ellora < Formula
    desc "Ellora - Multi-provider AI integration server"
    homepage "https://github.com/langdb/ellora"
    version "0.4.0-prerelease"  # Update this with your version

    on_macos do
      if Hardware::CPU.arm?
        url "https://github.com/langdb/ai-gateway/releases/download/v0.4.0-prerelease-1/ai-gateway-macos-aarch64"
        sha256 "2fb90c4c97589745abcdd7111fc3ae6461a846cca162cd1ab3ca433e32712014"  # Run: shasum -a 256 ai-gateway-aarch64
      else
        url "https://github.com/langdb/ai-gateway/releases/download/v0.4.0-prerelease-1/ai-gateway-macos-x86_64"
        sha256 "54b0e35cce59fa6b143b126224ee7ddee7e3785d0348f509ea75e654ed8ed36b"  # Run: shasum -a 256 ai-gateway-x86_64
      end
    end

    on_linux do
    #   if Hardware::CPU.arm?
    #     url "https://github.com/langdb/ai-gateway/releases/download/v0.3.2/ai-gateway-linux-aarch64"
    #     sha256 "f9dbe7dfbe1f7a6a817f0d3a674d54ad07062496e5753106d42d916ef450b7b2"  # Run: shasum -a 256 ai-gateway-aarch64
    #   else
        url "https://github.com/langdb/ai-gateway/releases/download/v0.4.0-prerelease-1/ai-gateway-linux-x86_64"
        sha256 "2adcc362db60aae2e8ddd5b7a54061bd5be013b092d41e1d3b52c4cf27b39bd1"  # Run: shasum -a 256 ai-gateway-x86_64
    #   end
    end

    def install
      if OS.mac?
        if Hardware::CPU.arm?
          bin.install "ai-gateway-macos-aarch64" => "ellora"
        else
          bin.install "ai-gateway-macos-x86_64" => "ellora"
        end
      elsif OS.linux?
        bin.install "ai-gateway-linux-x86_64" => "ellora"
      end
    end

    def caveats
      <<~EOS
        Server is running on port 8080 and UI can be accessed at http://localhost:8084
      EOS
    end

    test do
      system "#{bin}/ai-gateway", "--version"
    end
  end