#!/usr/bin/env ruby

require 'digest'
require 'net/http'
require 'uri'
require 'json'

class FormulaUpdater
  def initialize(tag_name, release_assets)
    @tag_name = tag_name
    @release_assets = release_assets
    @version = tag_name.gsub(/^v/, '')
    @base_url = "https://github.com/langdb/ellora/releases/download/#{tag_name}"
  end

  def update_formula
    puts "Updating formula for version #{@version}"
    
    # Calculate checksums for each platform
    checksums = calculate_checksums
    
    # Generate the new formula content
    formula_content = generate_formula_content(checksums)
    
    # Write to file
    File.write('Formula/ellora.rb', formula_content)
    puts "Formula updated successfully!"
  end

  private

  def calculate_checksums
    checksums = {}
    
    @release_assets.each do |asset|
      platform = determine_platform(asset)
      next unless platform
      
      puts "Calculating checksum for #{asset} (#{platform})"
      checksum = calculate_file_checksum(asset)
      checksums[platform] = checksum
    end
    
    checksums
  end

  def determine_platform(filename)
    case filename
    when /linux-x86_64/
      :linux_x86_64
    when /linux-aarch64/
      :linux_aarch64
    when /macos-x86_64/
      :macos_x86_64
    when /macos-aarch64/
      :macos_aarch64
    else
      nil
    end
  end

  def calculate_file_checksum(filepath)
    return nil unless File.exist?(filepath)
    
    content = File.read(filepath)
    Digest::SHA256.hexdigest(content)
  end

  def generate_formula_content(checksums)
    <<~RUBY
      class Ellora < Formula
          desc "Ellora - Multi-provider AI integration server"
          homepage "https://github.com/langdb/ellora"
          version "#{@version}"

          on_macos do
            if Hardware::CPU.arm?
              url "#{@base_url}/ai-gateway-macos-aarch64"
              sha256 "#{checksums[:macos_aarch64] || 'PLACEHOLDER_MACOS_ARM64_SHA256'}"
            else
              url "#{@base_url}/ai-gateway-macos-x86_64"
              sha256 "#{checksums[:macos_x86_64] || 'PLACEHOLDER_MACOS_X86_64_SHA256'}"
            end
          end

          on_linux do
            if Hardware::CPU.arm?
              url "#{@base_url}/ai-gateway-linux-aarch64"
              sha256 "#{checksums[:linux_aarch64] || 'PLACEHOLDER_LINUX_ARM64_SHA256'}"
            else
              url "#{@base_url}/ai-gateway-linux-x86_64"
              sha256 "#{checksums[:linux_x86_64] || 'PLACEHOLDER_LINUX_X86_64_SHA256'}"
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
            system "\#{bin}/ellora", "--version"
          end
        end
    RUBY
  end
end

# Main execution
if ARGV.length < 2
  puts "Usage: #{$0} <tag_name> <asset1> [asset2] ..."
  exit 1
end

tag_name = ARGV[0]
release_assets = ARGV[1..-1]

updater = FormulaUpdater.new(tag_name, release_assets)
updater.update_formula