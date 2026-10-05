Pod::Spec.new do |s|
  s.name           = 'FolioCore'
  s.version        = '0.1.0'
  s.summary        = 'Folio Rust core, Apple Vision OCR and PDFKit rendering for the mobile app'
  s.license        = 'UNLICENSED'
  s.author         = 'Folio'
  s.homepage       = 'https://github.com/NurhanMYZRM1/folio-expense'
  s.platforms      = { :ios => '16.4' }
  s.source         = { :git => '' }
  s.static_framework = true

  s.dependency 'ExpoModulesCore'

  s.source_files = '*.{swift,h}'
  s.public_header_files = 'folio.h'
  # Built by `npm run rust:ios` (scripts/build-rust-ios.sh) from ../src-tauri.
  s.vendored_frameworks = 'Frameworks/FolioFFI.xcframework'
  s.frameworks = 'Security', 'SystemConfiguration', 'CoreFoundation', 'Vision', 'PDFKit'
  s.pod_target_xcconfig = { 'DEFINES_MODULE' => 'YES' }
end
