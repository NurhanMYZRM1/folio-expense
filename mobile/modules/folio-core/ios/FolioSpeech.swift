import AVFoundation
import Speech

/// Short, user-initiated dictation. The transcript is returned for review; no
/// recording is saved. The platform service may need a network connection.
final class FolioSpeech {
  private var engine: AVAudioEngine?
  private var request: SFSpeechAudioBufferRecognitionRequest?
  private var task: SFSpeechRecognitionTask?
  private var finish: ((Result<String, Error>) -> Void)?
  private var partial = ""
  private var tapped = false
  private func failure(_ message: String) -> NSError {
    NSError(domain: "FolioSpeech", code: 1, userInfo: [NSLocalizedDescriptionKey: message])
  }
  func start(_ completion: @escaping (Result<String, Error>) -> Void) {
    guard finish == nil else { completion(.failure(failure("Dictation is already running."))); return }
    finish = completion
    SFSpeechRecognizer.requestAuthorization { [weak self] status in
      DispatchQueue.main.async {
        guard let self, self.finish != nil else { return }
        guard status == .authorized else { self.end(.failure(self.failure("Speech permission was denied. Type your transaction instead."))); return }
        AVAudioSession.sharedInstance().requestRecordPermission { granted in
          DispatchQueue.main.async {
            guard self.finish != nil else { return }
            guard granted else { self.end(.failure(self.failure("Microphone permission was denied. Type your transaction instead."))); return }
            self.record()
          }
        }
      }
    }
  }
  private func record() {
    guard let recognizer = SFSpeechRecognizer(locale: Locale.current), recognizer.isAvailable else {
      end(.failure(failure("Speech recognition is unavailable. Type your transaction instead."))); return
    }
    do {
      let session = AVAudioSession.sharedInstance()
      try session.setCategory(.record, mode: .measurement, options: .duckOthers)
      try session.setActive(true, options: .notifyOthersOnDeactivation)
      let engine = AVAudioEngine()
      let request = SFSpeechAudioBufferRecognitionRequest()
      self.engine = engine; self.request = request; partial = ""
      request.shouldReportPartialResults = true
      let node = engine.inputNode
      node.installTap(onBus: 0, bufferSize: 1024, format: node.outputFormat(forBus: 0)) { buffer, _ in request.append(buffer) }
      tapped = true
      task = recognizer.recognitionTask(with: request) { [weak self] result, error in
        DispatchQueue.main.async {
          guard let self, self.finish != nil else { return }
          if let result { self.partial = result.bestTranscription.formattedString }
          if result?.isFinal == true { self.end(.success(self.partial)) }
          else if error != nil { self.end(.failure(self.failure("Speech recognition could not finish. Type your transaction instead."))) }
        }
      }
      engine.prepare(); try engine.start()
      DispatchQueue.main.asyncAfter(deadline: .now() + 8) { [weak self, weak request] in
        guard self?.request === request else { return }
        self?.engine?.stop(); request?.endAudio()
      }
      DispatchQueue.main.asyncAfter(deadline: .now() + 15) { [weak self, weak request] in
        guard let self, self.request === request, self.finish != nil else { return }
        if self.partial.isEmpty { self.end(.failure(self.failure("No speech was recognized. Type your transaction instead."))) }
        else { self.end(.success(self.partial)) }
      }
    } catch { end(.failure(failure("The microphone could not start. Type your transaction instead."))) }
  }
  func cancel() { end(.failure(failure("Dictation cancelled."))) }
  private func end(_ result: Result<String, Error>) {
    guard let completion = finish else { return }
    finish = nil
    engine?.stop()
    if tapped { engine?.inputNode.removeTap(onBus: 0); tapped = false }
    request?.endAudio(); task?.cancel()
    engine = nil; request = nil; task = nil
    try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
    completion(result)
  }
}
