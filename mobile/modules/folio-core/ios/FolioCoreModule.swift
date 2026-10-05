import ExpoModulesCore
import PDFKit
import UIKit
import Vision

/// Bridges the Rust core (src-tauri `mobile` feature) and the two jobs the
/// desktop does in the webview with tesseract.js and pdf.js: reading receipt
/// text (Apple Vision) and rendering PDF pages (PDFKit).
public class FolioCoreModule: Module {
  private let queue = DispatchQueue(label: "folio.core", qos: .userInitiated, attributes: .concurrent)

  public func definition() -> ModuleDefinition {
    Name("FolioCore")

    /// Application Support/expense-app — the same folder name the desktop uses.
    Function("dataRoot") { () -> String in
      let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
      let root = base.appendingPathComponent("expense-app", isDirectory: true)
      try? FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
      return root.path
    }

    AsyncFunction("open") { (root: String) -> String in
      return Self.take(root.withCString { folio_open($0) })
    }.runOnQueue(queue)

    AsyncFunction("invoke") { (command: String, args: String) -> String in
      return Self.take(command.withCString { c in args.withCString { a in folio_invoke(c, a) } })
    }.runOnQueue(queue)

    AsyncFunction("recognizeText") { (path: String) throws -> [String: Any] in
      return try Self.recognize(path: path)
    }.runOnQueue(queue)

    AsyncFunction("renderPdfPages") { (path: String, maxPages: Int, maxSide: Double) throws -> [String] in
      return try Self.renderPdf(path: path, maxPages: maxPages, maxSide: CGFloat(maxSide))
    }.runOnQueue(queue)
  }

  private static func take(_ ptr: UnsafeMutablePointer<CChar>?) -> String {
    guard let ptr else { return #"{"error":{"code":"InternalError","message":"No response from the core."}}"# }
    defer { folio_string_free(ptr) }
    return String(cString: ptr)
  }

  // MARK: Text recognition

  /// Receipt text in reading order. Observations on the same visual line are
  /// joined left to right so "TOTAL ... 84.50" stays on one line, which the
  /// extraction heuristics in Rust rely on (the desktop gets the same from
  /// tesseract's single-column mode).
  private static func recognize(path: String) throws -> [String: Any] {
    guard let image = UIImage(contentsOfFile: path)?.cgImage else {
      throw Exception(name: "ReceiptUnreadable", description: "The receipt image could not be opened.")
    }
    let request = VNRecognizeTextRequest()
    request.recognitionLevel = .accurate
    request.usesLanguageCorrection = false
    try VNImageRequestHandler(cgImage: image, options: [:]).perform([request])
    let observations = (request.results ?? []).compactMap { obs -> (CGRect, String, Float)? in
      guard let best = obs.topCandidates(1).first else { return nil }
      return (obs.boundingBox, best.string, best.confidence)
    }
    var lines: [[(CGRect, String, Float)]] = []
    for obs in observations.sorted(by: { $0.0.midY > $1.0.midY }) {
      if let i = lines.firstIndex(where: { line in
        guard let first = line.first else { return false }
        return abs(first.0.midY - obs.0.midY) < max(first.0.height, obs.0.height) * 0.5
      }) {
        lines[i].append(obs)
      } else {
        lines.append([obs])
      }
    }
    let text = lines
      .map { $0.sorted(by: { $0.0.minX < $1.0.minX }).map { $0.1 }.joined(separator: "   ") }
      .joined(separator: "\n")
    var weighted: Float = 0
    var length = 0
    for obs in observations {
      weighted += obs.2 * Float(obs.1.count)
      length += obs.1.count
    }
    return ["text": text, "confidence": length > 0 ? Double(weighted / Float(length) * 100) : NSNull()]
  }

  // MARK: PDF pages

  private static func renderPdf(path: String, maxPages: Int, maxSide: CGFloat) throws -> [String] {
    guard let document = PDFDocument(url: URL(fileURLWithPath: path)) else {
      throw Exception(name: "PdfUnreadable", description: "This PDF could not be opened. It may be encrypted or damaged.")
    }
    let dir = FileManager.default.temporaryDirectory.appendingPathComponent("folio-pages-\(UUID().uuidString)", isDirectory: true)
    try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
    var out: [String] = []
    for index in 0..<min(document.pageCount, maxPages) {
      guard let page = document.page(at: index) else { continue }
      let bounds = page.bounds(for: .mediaBox)
      let scale = min(2.5, maxSide / max(bounds.width, bounds.height))
      let size = CGSize(width: bounds.width * scale, height: bounds.height * scale)
      let image = UIGraphicsImageRenderer(size: size).image { ctx in
        UIColor.white.setFill()
        ctx.fill(CGRect(origin: .zero, size: size))
        ctx.cgContext.translateBy(x: 0, y: size.height)
        ctx.cgContext.scaleBy(x: scale, y: -scale)
        page.draw(with: .mediaBox, to: ctx.cgContext)
      }
      let file = dir.appendingPathComponent("page-\(index + 1).jpg")
      guard let data = image.jpegData(compressionQuality: 0.85) else { continue }
      try data.write(to: file)
      out.append(file.path)
    }
    return out
  }
}
