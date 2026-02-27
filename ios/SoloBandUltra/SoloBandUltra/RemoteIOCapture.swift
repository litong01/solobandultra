// RemoteIOCapture.swift
//
// Microphone capture using a RemoteIO Audio Unit (input only), matching the approach
// used by the cpal crate on iOS: separate low-level Audio Unit for input so that
// AVAudioEngine playback is never reconfigured when capture starts.
//
// See: https://github.com/RustAudio/cpal/blob/master/src/host/coreaudio/ios/mod.rs

import Foundation
import AVFoundation
import AudioToolbox

/// Captures microphone input via a RemoteIO Audio Unit configured for input only.
/// Does not use AVAudioEngine, so it can run alongside engine-based playback without
/// stopping it. Session must already be configured (e.g. .playAndRecord) by the app.
final class RemoteIOCapture {
    private var audioUnit: AudioUnit?
    private let sampleRate: Double
    private let handler: (AVAudioPCMBuffer) -> Void
    private let inputBus: UInt32 = 1
    private let outputBus: UInt32 = 0
    /// Pre-allocated buffer for AudioUnitRender (real-time callback cannot allocate).
    private let maxFrames = 4096
    private var renderBuffer: UnsafeMutablePointer<Float>?

    /// - Parameters:
    ///   - sampleRate: Desired capture sample rate (e.g. 48000). Session should match.
    ///   - handler: Called on the audio thread with each captured buffer (mono Float).
    init(sampleRate: Double = 48000, handler: @escaping (AVAudioPCMBuffer) -> Void) {
        self.sampleRate = sampleRate
        self.handler = handler
        self.renderBuffer = UnsafeMutablePointer<Float>.allocate(capacity: maxFrames)
    }

    /// Start capture. Session must be active and have record permission.
    func start() throws {
        try ensureSessionActive()
        if audioUnit == nil {
            try createAndConfigureUnit()
        }
        guard let au = audioUnit else { return }
        try check(AudioUnitInitialize(au))
        try check(AudioOutputUnitStart(au))
    }

    /// Stop capture.
    func stop() {
        guard let au = audioUnit else { return }
        AudioOutputUnitStop(au)
        AudioUnitUninitialize(au)
        audioUnit = nil
    }

    deinit {
        stop()
        renderBuffer?.deallocate()
        renderBuffer = nil
    }

    // MARK: - Setup

    private func ensureSessionActive() throws {
        try AVAudioSession.sharedInstance().setActive(true)
        // Optionally request a buffer duration; skip if you don't want to affect shared session.
        let session = AVAudioSession.sharedInstance()
        let preferredDuration = 4096.0 / sampleRate
        try? session.setPreferredIOBufferDuration(preferredDuration)
    }

    private func createAndConfigureUnit() throws {
        var desc = AudioComponentDescription(
            componentType: kAudioUnitType_Output,
            componentSubType: kAudioUnitSubType_RemoteIO,
            componentManufacturer: kAudioUnitManufacturer_Apple,
            componentFlags: 0,
            componentFlagsMask: 0
        )
        guard let comp = AudioComponentFindNext(nil, &desc) else {
            throw NSError(domain: "RemoteIOCapture", code: -1, userInfo: [NSLocalizedDescriptionKey: "RemoteIO component not found"])
        }
        var unit: AudioUnit?
        try check(AudioComponentInstanceNew(comp, &unit))
        guard let au = unit else {
            throw NSError(domain: "RemoteIOCapture", code: -1, userInfo: [NSLocalizedDescriptionKey: "AudioComponentInstanceNew failed"])
        }
        self.audioUnit = au

        // Enable input (bus 1), disable output (bus 0) — same as cpal configure_for_recording
        var one: UInt32 = 1
        try check(AudioUnitSetProperty(au, kAudioOutputUnitProperty_EnableIO, kAudioUnitScope_Input, inputBus, &one, UInt32(MemoryLayout<UInt32>.size)))
        var zero: UInt32 = 0
        try check(AudioUnitSetProperty(au, kAudioOutputUnitProperty_EnableIO, kAudioUnitScope_Output, outputBus, &zero, UInt32(MemoryLayout<UInt32>.size)))

        // Stream format: mono Float32 at sampleRate (output scope of input element = data from mic)
        var asbd = AudioStreamBasicDescription(
            mSampleRate: sampleRate,
            mFormatID: kAudioFormatLinearPCM,
            mFormatFlags: kAudioFormatFlagIsFloat | kAudioFormatFlagIsPacked,
            mBytesPerPacket: UInt32(MemoryLayout<Float>.size),
            mFramesPerPacket: 1,
            mBytesPerFrame: UInt32(MemoryLayout<Float>.size),
            mChannelsPerFrame: 1,
            mBitsPerChannel: UInt32(8 * MemoryLayout<Float>.size),
            mReserved: 0
        )
        try check(AudioUnitSetProperty(au, kAudioUnitProperty_StreamFormat, kAudioUnitScope_Output, inputBus, &asbd, UInt32(MemoryLayout<AudioStreamBasicDescription>.size)))

        // Input callback — we get mic data here
        let ctx = Unmanaged.passUnretained(self).toOpaque()
        var callbackStruct = AURenderCallbackStruct(
            inputProc: inputCallback,
            inputProcRefCon: ctx
        )
        try check(AudioUnitSetProperty(au, kAudioOutputUnitProperty_SetInputCallback, kAudioUnitScope_Global, inputBus, &callbackStruct, UInt32(MemoryLayout<AURenderCallbackStruct>.size)))
    }

    private func check(_ status: OSStatus) throws {
        guard status == noErr else {
            throw NSError(domain: "RemoteIOCapture", code: Int(status), userInfo: [NSLocalizedDescriptionKey: "Audio Unit error \(status)"])
        }
    }

    // MARK: - Callback

    fileprivate func processInput(ioActionFlags: UnsafeMutablePointer<AudioUnitRenderActionFlags>, timeStamp: UnsafePointer<AudioTimeStamp>, busNumber: UInt32, frameCount: UInt32) {
        guard let au = audioUnit, let buf = renderBuffer, frameCount > 0, frameCount <= UInt32(maxFrames) else { return }
        var bufferList = AudioBufferList(
            mNumberBuffers: 1,
            mBuffers: AudioBuffer(
                mNumberChannels: 1,
                mDataByteSize: frameCount * UInt32(MemoryLayout<Float>.size),
                mData: UnsafeMutableRawPointer(buf)
            )
        )
        let status = AudioUnitRender(au, ioActionFlags, timeStamp, busNumber, frameCount, &bufferList)
        guard status == noErr else { return }
        guard let format = AVAudioFormat(standardFormatWithSampleRate: sampleRate, channels: 1),
              let pcmBuffer = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: AVAudioFrameCount(frameCount)) else { return }
        pcmBuffer.frameLength = AVAudioFrameCount(frameCount)
        if let channelData = pcmBuffer.floatChannelData?[0] {
            for i in 0..<Int(frameCount) {
                channelData[i] = buf[i]
            }
        }
        handler(pcmBuffer)
    }
}

private func inputCallback(
    inRefCon: UnsafeMutableRawPointer,
    ioActionFlags: UnsafeMutablePointer<AudioUnitRenderActionFlags>,
    inTimeStamp: UnsafePointer<AudioTimeStamp>,
    inBusNumber: UInt32,
    inNumberFrames: UInt32,
    ioData: UnsafeMutablePointer<AudioBufferList>?
) -> OSStatus {
    let capture = Unmanaged<RemoteIOCapture>.fromOpaque(inRefCon).takeUnretainedValue()
    capture.processInput(ioActionFlags: ioActionFlags, timeStamp: inTimeStamp, busNumber: inBusNumber, frameCount: inNumberFrames)
    return noErr
}
