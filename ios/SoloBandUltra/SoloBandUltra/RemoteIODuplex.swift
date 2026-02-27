// RemoteIODuplex.swift
//
// Full-duplex RemoteIO: one Audio Unit with both input and output enabled.
// Playback is driven from a pre-loaded buffer in the render callback; capture
// is delivered via the input callback. The session has a single client, so
// iOS should not reconfigure and stop playback when capture is active.
//
// Use this when Feedback is on; do not use AVAudioEngine in that case.

import Foundation
import AVFoundation
import AudioToolbox

/// One RemoteIO unit for both playback (from buffer) and capture (to handler).
/// Playback at 1.0 speed only in this path; mute and position are supported.
final class RemoteIODuplex {
    private var audioUnit: AudioUnit?
    private let sampleRate: Double
    private let inputBus: UInt32 = 1
    private let outputBus: UInt32 = 0

    /// Playback: stereo Float interleaved. Owned so pointer stays valid.
    private let playbackBuffer: [Float]
    private let totalFrames: Int
    private var playHead: Int = 0
    private var isPlayingFlag: Bool = false
    private var muteFlag: Bool = false
    private let playHeadLock = NSLock()

    private var onPlaybackFinished: (() -> Void)?
    private let captureHandler: (AVAudioPCMBuffer) -> Void

    private let maxInputFrames = 4096
    private var inputRenderBuffer: UnsafeMutablePointer<Float>?

    init(
        sampleRate: Double,
        playbackBuffer: [Float],
        totalFrames: Int,
        startFrame: Int,
        isMuted: Bool,
        onPlaybackFinished: @escaping () -> Void,
        captureHandler: @escaping (AVAudioPCMBuffer) -> Void
    ) {
        self.sampleRate = sampleRate
        self.playbackBuffer = playbackBuffer
        self.totalFrames = totalFrames
        self.playHead = startFrame
        self.muteFlag = isMuted
        self.onPlaybackFinished = onPlaybackFinished
        self.captureHandler = captureHandler
        self.inputRenderBuffer = UnsafeMutablePointer<Float>.allocate(capacity: maxInputFrames)
    }

    deinit {
        stop()
        inputRenderBuffer?.deallocate()
        inputRenderBuffer = nil
    }

    var isPlaying: Bool {
        get { playHeadLock.lock(); defer { playHeadLock.unlock() }; return isPlayingFlag }
        set { playHeadLock.lock(); isPlayingFlag = newValue; playHeadLock.unlock() }
    }

    var isMuted: Bool {
        get { playHeadLock.lock(); defer { playHeadLock.unlock() }; return muteFlag }
        set { playHeadLock.lock(); muteFlag = newValue; playHeadLock.unlock() }
    }

    /// Current playback position in frames (for cursor). Safe to call from main.
    var currentFrame: Int {
        playHeadLock.lock()
        defer { playHeadLock.unlock() }
        return playHead
    }

    func setPlayHead(_ frame: Int) {
        playHeadLock.lock()
        playHead = min(max(0, frame), totalFrames)
        playHeadLock.unlock()
    }

    func start() throws {
        try AVAudioSession.sharedInstance().setActive(true)
        if audioUnit == nil {
            try createAndConfigureUnit()
        }
        guard let au = audioUnit else { return }
        playHeadLock.lock()
        isPlayingFlag = true
        playHeadLock.unlock()
        try check(AudioUnitInitialize(au))
        try check(AudioOutputUnitStart(au))
    }

    func stop() {
        guard let au = audioUnit else { return }
        AudioOutputUnitStop(au)
        AudioUnitUninitialize(au)
        audioUnit = nil
        playHeadLock.lock()
        isPlayingFlag = false
        playHeadLock.unlock()
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
            throw NSError(domain: "RemoteIODuplex", code: -1, userInfo: [NSLocalizedDescriptionKey: "RemoteIO not found"])
        }
        var unit: AudioUnit?
        try check(AudioComponentInstanceNew(comp, &unit))
        guard let au = unit else { throw NSError(domain: "RemoteIODuplex", code: -1, userInfo: [NSLocalizedDescriptionKey: "Failed to create unit"]) }
        self.audioUnit = au

        // Full duplex: enable BOTH input and output on the same unit
        var one: UInt32 = 1
        try check(AudioUnitSetProperty(au, kAudioOutputUnitProperty_EnableIO, kAudioUnitScope_Input, inputBus, &one, UInt32(MemoryLayout<UInt32>.size)))
        try check(AudioUnitSetProperty(au, kAudioOutputUnitProperty_EnableIO, kAudioUnitScope_Output, outputBus, &one, UInt32(MemoryLayout<UInt32>.size)))

        // Format: stereo Float for output (input scope of output element)
        var asbdOut = AudioStreamBasicDescription(
            mSampleRate: sampleRate,
            mFormatID: kAudioFormatLinearPCM,
            mFormatFlags: kAudioFormatFlagIsFloat | kAudioFormatFlagIsPacked,
            mBytesPerPacket: UInt32(2 * MemoryLayout<Float>.size),
            mFramesPerPacket: 1,
            mBytesPerFrame: UInt32(2 * MemoryLayout<Float>.size),
            mChannelsPerFrame: 2,
            mBitsPerChannel: UInt32(8 * MemoryLayout<Float>.size),
            mReserved: 0
        )
        try check(AudioUnitSetProperty(au, kAudioUnitProperty_StreamFormat, kAudioUnitScope_Input, outputBus, &asbdOut, UInt32(MemoryLayout<AudioStreamBasicDescription>.size)))

        // Format: mono Float for input (output scope of input element)
        var asbdIn = AudioStreamBasicDescription(
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
        try check(AudioUnitSetProperty(au, kAudioUnitProperty_StreamFormat, kAudioUnitScope_Output, inputBus, &asbdIn, UInt32(MemoryLayout<AudioStreamBasicDescription>.size)))

        // Render callback (output)
        let ctx = Unmanaged.passUnretained(self).toOpaque()
        var renderStruct = AURenderCallbackStruct(inputProc: renderCallback, inputProcRefCon: ctx)
        try check(AudioUnitSetProperty(au, kAudioUnitProperty_SetRenderCallback, kAudioUnitScope_Input, outputBus, &renderStruct, UInt32(MemoryLayout<AURenderCallbackStruct>.size)))

        // Input callback (capture)
        var inputStruct = AURenderCallbackStruct(inputProc: inputCallback, inputProcRefCon: ctx)
        try check(AudioUnitSetProperty(au, kAudioOutputUnitProperty_SetInputCallback, kAudioUnitScope_Global, inputBus, &inputStruct, UInt32(MemoryLayout<AURenderCallbackStruct>.size)))
    }

    private func check(_ status: OSStatus) throws {
        guard status == noErr else {
            throw NSError(domain: "RemoteIODuplex", code: Int(status), userInfo: [NSLocalizedDescriptionKey: "Audio Unit error \(status)"])
        }
    }

    fileprivate func renderOutput(ioData: UnsafeMutablePointer<AudioBufferList>, frameCount: UInt32) {
        let list = UnsafeMutableAudioBufferListPointer(ioData)
        guard let outBuf = list.first, let outPtr = outBuf.mData?.assumingMemoryBound(to: Float.self) else { return }
        let n = Int(frameCount)
        let stereo = n * 2

        playHeadLock.lock()
        let playing = isPlayingFlag
        let muted = muteFlag
        var head = playHead
        playHeadLock.unlock()

        if !playing || muted {
            memset(outPtr, 0, stereo * MemoryLayout<Float>.size)
            return
        }

        playbackBuffer.withUnsafeBufferPointer { buf in
            let base = buf.baseAddress!
            let start = head * 2
            let available = (totalFrames - head) * 2
            if available >= stereo {
                memcpy(outPtr, base + start, stereo * MemoryLayout<Float>.size)
                head += n
            } else {
                let copy = available
                if copy > 0 {
                    memcpy(outPtr, base + start, copy * MemoryLayout<Float>.size)
                }
                memset(outPtr + copy, 0, (stereo - copy) * MemoryLayout<Float>.size)
                head = totalFrames
                DispatchQueue.main.async { [weak self] in
                    self?.onPlaybackFinished?()
                }
            }
        }

        playHeadLock.lock()
        playHead = head
        playHeadLock.unlock()
    }

    fileprivate func processInput(ioActionFlags: UnsafeMutablePointer<AudioUnitRenderActionFlags>, timeStamp: UnsafePointer<AudioTimeStamp>, busNumber: UInt32, frameCount: UInt32) {
        guard let au = audioUnit, let buf = inputRenderBuffer, frameCount > 0, frameCount <= UInt32(maxInputFrames) else { return }
        var bufferList = AudioBufferList(
            mNumberBuffers: 1,
            mBuffers: AudioBuffer(mNumberChannels: 1, mDataByteSize: frameCount * UInt32(MemoryLayout<Float>.size), mData: UnsafeMutableRawPointer(buf))
        )
        let status = AudioUnitRender(au, ioActionFlags, timeStamp, busNumber, frameCount, &bufferList)
        guard status == noErr else { return }
        guard let format = AVAudioFormat(standardFormatWithSampleRate: sampleRate, channels: 1),
              let pcm = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: AVAudioFrameCount(frameCount)) else { return }
        pcm.frameLength = AVAudioFrameCount(frameCount)
        if let ch = pcm.floatChannelData?[0] {
            for i in 0..<Int(frameCount) { ch[i] = buf[i] }
        }
        captureHandler(pcm)
    }
}

private func renderCallback(
    inRefCon: UnsafeMutableRawPointer,
    ioActionFlags: UnsafeMutablePointer<AudioUnitRenderActionFlags>,
    inTimeStamp: UnsafePointer<AudioTimeStamp>,
    inBusNumber: UInt32,
    inNumberFrames: UInt32,
    ioData: UnsafeMutablePointer<AudioBufferList>?
) -> OSStatus {
    guard let ioData = ioData else { return noErr }
    let duplex = Unmanaged<RemoteIODuplex>.fromOpaque(inRefCon).takeUnretainedValue()
    duplex.renderOutput(ioData: ioData, frameCount: inNumberFrames)
    return noErr
}

private func inputCallback(
    inRefCon: UnsafeMutableRawPointer,
    ioActionFlags: UnsafeMutablePointer<AudioUnitRenderActionFlags>,
    inTimeStamp: UnsafePointer<AudioTimeStamp>,
    inBusNumber: UInt32,
    inNumberFrames: UInt32,
    ioData: UnsafeMutablePointer<AudioBufferList>?
) -> OSStatus {
    let duplex = Unmanaged<RemoteIODuplex>.fromOpaque(inRefCon).takeUnretainedValue()
    duplex.processInput(ioActionFlags: ioActionFlags, timeStamp: inTimeStamp, busNumber: inBusNumber, frameCount: inNumberFrames)
    return noErr
}
