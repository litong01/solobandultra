#import "MbkExtractor.h"
#import <zlib.h>

// ── ZIP local-file header constants ───────────────────────────────────
static const uint32_t kLocalFileSig = 0x04034b50u;

static uint16_t le16(const uint8_t *b) {
    return (uint16_t)(b[0] | ((uint16_t)b[1] << 8));
}
static uint32_t le32(const uint8_t *b) {
    return (uint32_t)(  b[0]
                      | ((uint32_t)b[1] <<  8)
                      | ((uint32_t)b[2] << 16)
                      | ((uint32_t)b[3] << 24));
}

// Decompress raw DEFLATE (no zlib header/trailer) using zlib's inflate.
static NSData * _Nullable inflate_raw(const uint8_t *src,
                                      uLongf         srcLen,
                                      uLongf         uSizeHint,
                                      NSError       * _Nullable __autoreleasing *outErr)
{
    uLongf outCap = (uSizeHint > 0) ? uSizeHint : MAX(srcLen * 4, (uLongf)4096);
    NSMutableData *out = [NSMutableData dataWithLength:outCap];

    z_stream strm;
    memset(&strm, 0, sizeof(strm));
    strm.next_in  = (Bytef *)src;
    strm.avail_in = (uInt)srcLen;

    // -15 = raw DEFLATE (no zlib wrapper)
    if (inflateInit2(&strm, -15) != Z_OK) {
        if (outErr) {
            *outErr = [NSError errorWithDomain:@"MbkExtractor" code:1
                                     userInfo:@{NSLocalizedDescriptionKey: @"inflateInit2 failed"}];
        }
        return nil;
    }

    // Point inflate at the start of the output buffer before the loop.
    strm.next_out  = (Bytef *)out.mutableBytes;
    strm.avail_out = (uInt)outCap;

    int status;
    do {
        if (strm.avail_out == 0) {
            // Output buffer full — double it and resume writing after what we've written so far.
            uLong written = strm.total_out;
            outCap *= 2;
            [out setLength:outCap];
            strm.next_out  = (Bytef *)out.mutableBytes + written;
            strm.avail_out = (uInt)(outCap - written);
        }
        status = inflate(&strm, Z_SYNC_FLUSH);
    } while (status == Z_OK);

    inflateEnd(&strm);

    if (status != Z_STREAM_END) {
        if (outErr) {
            *outErr = [NSError errorWithDomain:@"MbkExtractor" code:2
                                     userInfo:@{NSLocalizedDescriptionKey:
                                                    [NSString stringWithFormat:@"inflate error %d", status]}];
        }
        return nil;
    }

    [out setLength:strm.total_out];
    return out;
}

// ── Public API ────────────────────────────────────────────────────────

NSError * _Nullable mbk_extract(NSData * _Nonnull zipData,
                                NSURL  * _Nonnull destDirectory)
{
    NSFileManager *fm = [NSFileManager defaultManager];
    NSError *err = nil;

    if (![fm createDirectoryAtURL:destDirectory
       withIntermediateDirectories:YES
                        attributes:nil
                             error:&err]) {
        return err;
    }

    const uint8_t *bytes = (const uint8_t *)zipData.bytes;
    NSUInteger     total = zipData.length;
    NSUInteger     offset = 0;

    while (offset + 30 <= total) {
        uint32_t sig = le32(bytes + offset);
        if (sig != kLocalFileSig) break;   // reached central directory or end

        // Local file header layout (all fields little-endian):
        //   offset  size  field
        //    0       4    local file header signature
        //    4       2    version needed
        //    6       2    general purpose bit flag
        //    8       2    compression method (0=store, 8=deflate)
        //   10       2    last mod file time
        //   12       2    last mod file date
        //   14       4    crc-32
        //   18       4    compressed size
        //   22       4    uncompressed size
        //   26       2    file name length
        //   28       2    extra field length
        //   30       ?    file name
        //   30+fn   ?    extra field
        //   30+fn+ex ?   compressed data

        uint16_t method      = le16(bytes + offset +  8);
        uint32_t cSize       = le32(bytes + offset + 18);
        uint32_t uSize       = le32(bytes + offset + 22);
        uint16_t nameLen     = le16(bytes + offset + 26);
        uint16_t extraLen    = le16(bytes + offset + 28);

        NSUInteger nameStart  = offset + 30;
        NSUInteger nameEnd    = nameStart + nameLen;
        NSUInteger dataStart  = nameEnd   + extraLen;
        NSUInteger dataEnd    = dataStart + cSize;

        if (nameEnd > total || dataEnd > total) break;  // truncated archive

        // Try UTF-8 first (covers ASCII, Chinese, and other Unicode); fall back to
        // Latin-1 for legacy archives; skip entries whose names cannot be decoded.
        NSString *fileName =
            [[NSString alloc] initWithBytes:bytes + nameStart
                                     length:nameLen
                                   encoding:NSUTF8StringEncoding]
            ?: [[NSString alloc] initWithBytes:bytes + nameStart
                                        length:nameLen
                                      encoding:NSISOLatin1StringEncoding];

        offset = dataEnd;   // advance before any continue/return

        // Skip directory entries, undecodable names, and empty names.
        if (!fileName || [fileName hasSuffix:@"/"] || fileName.length == 0) continue;

        // Skip macOS resource-fork entries (__MACOSX/... and ._... files).
        if ([fileName hasPrefix:@"__MACOSX/"] || [fileName.lastPathComponent hasPrefix:@"._"]) continue;

        NSURL *destFile = [destDirectory URLByAppendingPathComponent:fileName];
        NSURL *parent   = [destFile URLByDeletingLastPathComponent];

        if (![fm createDirectoryAtURL:parent
           withIntermediateDirectories:YES
                            attributes:nil
                                 error:NULL]) {
            return [NSError errorWithDomain:@"MbkExtractor" code:5
                                   userInfo:@{NSLocalizedDescriptionKey:
                                                  [NSString stringWithFormat:@"Cannot create directory for %@", fileName]}];
        }

        NSData *output = nil;

        if (method == 0) {
            // Stored — direct copy
            output = [NSData dataWithBytes:bytes + dataStart length:cSize];

        } else if (method == 8) {
            // Deflated — raw DEFLATE decompression
            NSError *inflateErr = nil;
            output = inflate_raw(bytes + dataStart, (uLongf)cSize, (uLongf)uSize, &inflateErr);
            if (!output) return inflateErr;

        } else {
            return [NSError errorWithDomain:@"MbkExtractor" code:3
                                   userInfo:@{NSLocalizedDescriptionKey:
                                                  [NSString stringWithFormat:
                                                   @"Unsupported ZIP compression method %u in '%@'",
                                                   (unsigned)method, fileName]}];
        }

        if (![output writeToURL:destFile atomically:YES]) {
            return [NSError errorWithDomain:@"MbkExtractor" code:4
                                   userInfo:@{NSLocalizedDescriptionKey:
                                                  [NSString stringWithFormat:@"Failed to write '%@'", fileName]}];
        }
    }

    return nil;  // success
}
