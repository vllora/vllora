class Ellora < Formula
    desc "Ellora - Multi-provider AI integration server"
    homepage "https://github.com/langdb/ellora"
    version "1.0.0-test"

    on_macos do
      if Hardware::CPU.arm?
        url "https://github.com/langdb/ellora/releases/download/v1.0.0-test/ai-gateway-macos-aarch64"
        sha256 "6a42d048d0f67655199fa38f33814d8da4a663649309d076bc33981ed6374292"
      else
        url "https://github.com/langdb/ellora/releases/download/v1.0.0-test/ai-gateway-macos-x86_64"
        sha256 "b3674cfbcd5966dfc7a5861ad47c1b5a7726237e6b26eac5ab933cd7c300850f"
      end
    end

    on_linux do
      if Hardware::CPU.arm?
        url "https://github.com/langdb/ellora/releases/download/v1.0.0-test/ai-gateway-linux-aarch64"
        sha256 "f8ca9364108733ddd8de2f35995100d9e42bbd852b26bbef8653475b37d8df80"
      else
        url "https://github.com/langdb/ellora/releases/download/v1.0.0-test/ai-gateway-linux-x86_64"
        sha256 "5d4547ee50748ac16ead1b5afd33861987a29cd55dbe93a68effa1f003dc454f"
      end
    end

    def install
      if OS.mac?
        if Hardware::CPU.arm?
          bin.install "ai-gateway-macos-aarch64" => "ellora"
        else
          bin.install "ai-gateway-macos-x86_64" => "ellora"
        end
      elsif OS.linux?
        if Hardware::CPU.arm?
          bin.install "ai-gateway-linux-aarch64" => "ellora"
        else
          bin.install "ai-gateway-linux-x86_64" => "ellora"
        end
      end
    end

    def caveats
      <<~EOS
        Server is running on port 8080 and UI can be accessed at http://localhost:8084
      EOS
    end

    test do
      system "#{bin}/ellora", "--version"
    end
  end
