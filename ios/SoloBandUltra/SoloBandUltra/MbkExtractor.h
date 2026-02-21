#ifndef MbkExtractor_h
#define MbkExtractor_h

#import <Foundation/Foundation.h>

/// Unzips a .mbk (ZIP) archive from @p zipData into @p destDirectory,
/// creating intermediate directories as needed.
///
/// @return nil on success; an NSError describing the failure otherwise.
NSError * _Nullable mbk_extract(NSData * _Nonnull zipData,
                                NSURL  * _Nonnull destDirectory);

#endif /* MbkExtractor_h */
