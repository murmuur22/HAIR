// Held-byte macOS still adapter. Scoped policy, NOT arbitrary MP4 containment,
// native codec heap/work bounds, interruptible cancellation or a drain guarantee.
#import <Foundation/Foundation.h>
#import <AVFoundation/AVFoundation.h>
#import <CoreMedia/CoreMedia.h>
#import <CoreGraphics/CoreGraphics.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <math.h>
#include <time.h>

static const NSUInteger InputCap=1024*1024, ChunkCap=16384, RequestCap=64;
static const NSUInteger DeliveryCap=8*1024*1024, ChunksCap=512;
static double Now(void) { struct timespec t; clock_gettime(CLOCK_MONOTONIC,&t); return t.tv_sec+t.tv_nsec/1e9; }

typedef struct { unsigned char *pixels; size_t length; uint32_t width, height; int code; } HairStillResult;
@interface HairStillLedger : NSObject
@property(nonatomic) NSUInteger contexts, loaders, sources, sourceBytes, chunks, chunkBytes;
@end
@implementation HairStillLedger
@end
// Source and chunks each capture only the independently owned ledger. No Rust
// callback/pointer and no back-edge from ledger to NSData/context/loader.
static NSData *OwnedBytes(const void *bytes, NSUInteger length, HairStillLedger *ledger, BOOL source) {
    @synchronized(ledger) {
        if(source) {
            if(ledger.sources || !length || length>InputCap) return nil;
            ledger.sources++; ledger.sourceBytes+=length;
        } else {
            if(ledger.chunks>=ChunksCap || length>DeliveryCap-ledger.chunkBytes) return nil;
            ledger.chunks++; ledger.chunkBytes+=length;
        }
    }
    void *memory=malloc(length);
    if(!memory) {
        @synchronized(ledger) {
            if(source) {ledger.sources--;ledger.sourceBytes-=length;}
            else {ledger.chunks--;ledger.chunkBytes-=length;}
        }
        return nil;
    }
    memcpy(memory,bytes,length);
    return [[NSData alloc] initWithBytesNoCopy:memory length:length deallocator:^(void *p, NSUInteger n) {
        free(p);
        @synchronized(ledger) {
            if(source) {ledger.sources--;ledger.sourceBytes-=n;}
            else {ledger.chunks--;ledger.chunkBytes-=n;}
        }
    }];
}
static NSString *RangePlan(NSUInteger size, long long requested, long long current, NSInteger length,
                           BOOL all, uint64_t *start, uint64_t *end) {
    if(requested<0 || current<requested || (uint64_t)requested>size || (uint64_t)current>size) return @"offset";
    *start=(uint64_t)current;
    if(all) *end=size;
    else if(length<0 || (uint64_t)length>UINT64_MAX-(uint64_t)requested) return @"length";
    else *end=MIN((uint64_t)size,(uint64_t)requested+(uint64_t)length);
    return *start>*end ? @"current_beyond_end" : nil;
}
@interface HairStillLoader : NSObject <AVAssetResourceLoaderDelegate>
@property(nonatomic,strong) NSData *bytes;
@property(nonatomic,strong) HairStillLedger *ledger;
@property(nonatomic,strong) dispatch_queue_t queue;
@property(nonatomic,copy) NSString *token;
@property(nonatomic) NSUInteger requests, delivered, chunks, refusals;
@property(nonatomic) BOOL stopped;
@property(nonatomic) double deadline;
@end
@implementation HairStillLoader
- (void)dealloc { @synchronized(_ledger) { _ledger.loaders--; } }
- (BOOL)resourceLoader:(AVAssetResourceLoader *)resourceLoader shouldWaitForLoadingOfRequestedResource:(AVAssetResourceLoadingRequest *)request {
    (void)resourceLoader;
    HairStillLoader * __attribute__((objc_precise_lifetime)) owner=self;
    @synchronized(owner) {
        NSString *reason=nil;
        if(owner.requests>=RequestCap) reason=@"request_cap";
        else {
            owner.requests++;
            if(owner.stopped || Now()>=owner.deadline) reason=@"stopped";
            else if(![request.request.URL.absoluteString isEqualToString:owner.token]) reason=@"exact_source";
        }
        if(!reason && (request.request.HTTPBody!=nil || request.request.HTTPBodyStream!=nil ||
            (request.request.HTTPMethod && ![request.request.HTTPMethod isEqualToString:@"GET"]))) reason=@"method_or_body";
        if(!reason && request.cancelled) reason=@"cancelled";
        AVAssetResourceLoadingContentInformationRequest *info=request.contentInformationRequest;
        if(!reason && info) {
            if(info.allowedContentTypes.count && ![info.allowedContentTypes containsObject:@"public.mpeg-4"]) reason=@"type";
            else { info.contentType=@"public.mpeg-4";info.contentLength=(long long)owner.bytes.length;
                info.byteRangeAccessSupported=YES;info.entireLengthAvailableOnDemand=YES; }
        }
        AVAssetResourceLoadingDataRequest *d=request.dataRequest;
        uint64_t start=0,end=0;
        if(!reason && d) reason=RangePlan(owner.bytes.length,d.requestedOffset,d.currentOffset,d.requestedLength,d.requestsAllDataToEndOfResource,&start,&end);
        while(!reason && d && start<end) {
            if(owner.stopped || Now()>=owner.deadline || request.cancelled) {reason=@"cancel_or_window";break;}
            NSUInteger n=(NSUInteger)MIN((uint64_t)ChunkCap,end-start);
            if(owner.chunks>=ChunksCap || n>DeliveryCap-owner.delivered) {reason=@"delivery_budget";break;}
            @autoreleasepool {
                NSData *part=OwnedBytes((const unsigned char *)owner.bytes.bytes+(NSUInteger)start,n,owner.ledger,NO);
                if(!part) reason=@"live_backing_budget";
                else {
                    owner.chunks++;owner.delivered+=n;
                    [d respondWithData:part];start+=n;
                    if(d.currentOffset!=(long long)start) reason=@"current_offset";
                }
            }
        }
        if(reason && owner.refusals<NSUIntegerMax) owner.refusals++;
        if(!request.cancelled && !request.finished) {
            if(reason) [request finishLoadingWithError:[NSError errorWithDomain:@"HairStillRefusal" code:2 userInfo:nil]];
            else [request finishLoading];
        }
        return YES; // Never forward, redirect or fall back to another source.
    }
}
- (BOOL)resourceLoader:(AVAssetResourceLoader *)l shouldWaitForRenewalOfRequestedResource:(AVAssetResourceRenewalRequest *)r {(void)l;(void)r;return NO;}
- (BOOL)resourceLoader:(AVAssetResourceLoader *)l shouldWaitForResponseToAuthenticationChallenge:(NSURLAuthenticationChallenge *)c {(void)l;(void)c;return NO;}
@end
@interface HairStillSignal : NSObject
@property(nonatomic,strong) dispatch_semaphore_t semaphore;
@property(nonatomic) BOOL ok;
@end
@implementation HairStillSignal
- (instancetype)init { if((self=[super init])) _semaphore=dispatch_semaphore_create(0);return self; }
@end
@interface HairStillContext : NSObject {
@public CGImageRef image;
}
@property(nonatomic,strong) HairStillLedger *ledger;
@property(nonatomic,strong) AVURLAsset *asset;
@property(nonatomic,strong) AVAssetImageGenerator *generator;
@property(nonatomic,strong) HairStillLoader *loader;
@property(nonatomic,strong) HairStillSignal *signal;
@property(nonatomic) BOOL obsolete, received;
@property(nonatomic) CMTime actual;
@property(nonatomic,strong) NSError *error;
- (void)receive:(CGImageRef)borrowed actual:(CMTime)actual error:(NSError *)error;
- (void)retire;
@end
@implementation HairStillContext
- (void)dealloc { if(image) CGImageRelease(image);@synchronized(_ledger) {_ledger.contexts--;} }
- (void)receive:(CGImageRef)borrowed actual:(CMTime)actual error:(NSError *)error {
    CGImageRef owned=borrowed?CGImageRetain(borrowed):NULL;
    @synchronized(self) {
        if(_obsolete || _received) {if(owned) CGImageRelease(owned);}
        else {_received=YES;image=owned;_actual=actual;_error=error;}
    }
    dispatch_semaphore_signal(_signal.semaphore);
}
- (void)retire {
    @synchronized(self) {_obsolete=YES;if(image) {CGImageRelease(image);image=NULL;}}
    @synchronized(_loader) {_loader.stopped=YES;}
    [_generator cancelAllCGImageGeneration];[_asset cancelLoading];
    [_asset.resourceLoader setDelegate:nil queue:nil];
    _generator=nil;_asset=nil;_loader=nil;
    // No barrier/weak-nil/cancel call authorizes slot reuse.
}
@end
@interface HairStillSlot : NSObject
@property(nonatomic,strong) HairStillLedger *ledger;
@property(nonatomic,weak) HairStillContext *context;
@property(nonatomic,weak) HairStillLoader *loader;
@property(nonatomic,weak) AVURLAsset *asset;
@property(nonatomic,weak) AVAssetImageGenerator *generator;
@end
@implementation HairStillSlot
- (instancetype)init {if((self=[super init])) _ledger=[HairStillLedger new];return self;}
@end
static BOOL Available(HairStillSlot *slot) {
    BOOL absent=!slot.context && !slot.loader && !slot.asset && !slot.generator;
    HairStillLedger *l=slot.ledger;
    @synchronized(l) {return absent && !l.contexts && !l.loaders && !l.sources && !l.sourceBytes && !l.chunks && !l.chunkBytes;}
}
static BOOL LoadKeys(HairStillContext *c, id<AVAsynchronousKeyValueLoading> object, NSArray<NSString *> *keys, double deadline) {
    HairStillSignal *signal=[HairStillSignal new];
    [object loadValuesAsynchronouslyForKeys:keys completionHandler:^{
        HairStillContext * __attribute__((objc_precise_lifetime)) keep=c;
        BOOL ok=YES;
        for(NSString *key in keys) if([object statusOfValueForKey:key error:NULL]!=AVKeyValueStatusLoaded) ok=NO;
        @synchronized(signal) {signal.ok=ok;}
        (void)keep;dispatch_semaphore_signal(signal.semaphore);
    }];
    double left=deadline-Now();
    if(left<=0 || dispatch_semaphore_wait(signal.semaphore,dispatch_time(DISPATCH_TIME_NOW,(int64_t)(left*NSEC_PER_SEC)))) return NO;
    @synchronized(signal) {return signal.ok;}
}
// Caller serializes every slot API with its owned mutex. Only weak observations
// and independent ledger are retained by slot, never permanent thumbnail roots.
void *hair_still_slot_new(void) {
    @autoreleasepool {return (__bridge_retained void *)[HairStillSlot new];}
}
int hair_still_slot_available(void *handle) {
    @autoreleasepool {return handle && Available((__bridge HairStillSlot *)handle);}
}
void hair_still_slot_free(void *handle) {
    @autoreleasepool {HairStillSlot *slot=(__bridge_transfer HairStillSlot *)handle;(void)slot;}
}
void hair_still_pixels_free(void *pixels) {free(pixels);}

static int Decode(HairStillSlot *slot, const unsigned char *bytes, size_t length, HairStillResult *out) {
    if(!Available(slot)) return 1;
    if(!bytes || length<12 || length>InputCap || memcmp(bytes+4,"ftyp",4)) return 2;
    // Owned copy completes synchronously; no callback captures bytes or out.
    NSData *data=OwnedBytes(bytes,length,slot.ledger,YES);
    if(!data) return 3;
    HairStillContext *c=[HairStillContext new];c.ledger=slot.ledger;c.signal=[HairStillSignal new];
    @synchronized(slot.ledger) {slot.ledger.contexts++;}
    c.loader=[HairStillLoader new];c.loader.ledger=slot.ledger;c.loader.bytes=data;
    @synchronized(slot.ledger) {slot.ledger.loaders++;}
    c.loader.token=[@"hairstill-held://source/" stringByAppendingFormat:@"%@.mp4",NSUUID.UUID.UUIDString];
    c.loader.deadline=Now()+12;c.loader.queue=dispatch_queue_create("hair.still.held",DISPATCH_QUEUE_SERIAL);
    NSDictionary *options=@{AVURLAssetReferenceRestrictionsKey:@(AVAssetReferenceRestrictionForbidAll),
        AVURLAssetShouldSupportAliasDataReferencesKey:@NO,AVURLAssetAllowsCellularAccessKey:@NO,
        AVURLAssetAllowsExpensiveNetworkAccessKey:@NO,AVURLAssetAllowsConstrainedNetworkAccessKey:@NO,
        AVURLAssetPreferPreciseDurationAndTimingKey:@YES};
    c.asset=[[AVURLAsset alloc] initWithURL:[NSURL URLWithString:c.loader.token] options:options];
    c.asset.resourceLoader.preloadsEligibleContentKeys=NO;
    [c.asset.resourceLoader setDelegate:c.loader queue:c.loader.queue];
    slot.context=c;slot.loader=c.loader;slot.asset=c.asset;
    double deadline=c.loader.deadline;int code=0;
    if(!c.asset || c.asset.referenceRestrictions!=AVAssetReferenceRestrictionForbidAll ||
        c.asset.resourceLoader.delegate!=c.loader || c.asset.resourceLoader.delegateQueue!=c.loader.queue) code=4;
    if(!code && !LoadKeys(c,c.asset,@[@"tracks",@"duration"],deadline)) code=5;
    AVAssetTrack *track=nil;
    if(!code) {
        NSArray<AVAssetTrack *> *tracks=c.asset.tracks;
        if(tracks.count!=1) code=6;else track=tracks[0];
    }
    if(!code && !LoadKeys(c,track,@[@"mediaType",@"naturalSize",@"preferredTransform",@"formatDescriptions"],deadline)) code=5;
    uint32_t width=0,height=0;
    if(!code) {
        CGSize s=track.naturalSize;double duration=CMTimeGetSeconds(c.asset.duration);
        NSArray *formats=track.formatDescriptions;
        CMFormatDescriptionRef f=formats.count==1?(__bridge CMFormatDescriptionRef)formats[0]:NULL;
        if(![track.mediaType isEqual:AVMediaTypeVideo] || !isfinite(s.width) || !isfinite(s.height) ||
            s.width<1 || s.height<1 || s.width>160 || s.height>96 || floor(s.width)!=s.width || floor(s.height)!=s.height ||
            !CGAffineTransformIsIdentity(track.preferredTransform) || !isfinite(duration) || duration<=0 || duration>2.02 ||
            !f || CMFormatDescriptionGetMediaType(f)!=kCMMediaType_Video || CMFormatDescriptionGetMediaSubType(f)!=kCMVideoCodecType_H264) code=6;
        else {
            CMVideoDimensions d=CMVideoFormatDescriptionGetDimensions(f);
            width=(uint32_t)s.width;height=(uint32_t)s.height;
            if(d.width!=(int32_t)width || d.height!=(int32_t)height) code=6;
        }
    }
    if(!code) {
        c.generator=[[AVAssetImageGenerator alloc] initWithAsset:c.asset];slot.generator=c.generator;
        if(!c.generator) code=7;
        else {
            c.generator.maximumSize=CGSizeMake(width,height); // NOT an allocation guarantee.
            c.generator.appliesPreferredTrackTransform=YES;
            c.generator.apertureMode=AVAssetImageGeneratorApertureModeEncodedPixels;
            c.generator.requestedTimeToleranceBefore=kCMTimeZero;c.generator.requestedTimeToleranceAfter=kCMTimeZero;
            [c.generator generateCGImageAsynchronouslyForTime:kCMTimeZero completionHandler:^(CGImageRef image, CMTime actual, NSError *error) {
                [c receive:image actual:actual error:error];
            }];
            double left=deadline-Now();
            if(left<=0 || dispatch_semaphore_wait(c.signal.semaphore,dispatch_time(DISPATCH_TIME_NOW,(int64_t)(left*NSEC_PER_SEC)))) code=8;
            else { @synchronized(c) {
                if(!c.received || !c->image || c.error || !CMTIME_IS_NUMERIC(c.actual) || fabs(CMTimeGetSeconds(c.actual))>0.001) code=9;
                else if(CGImageGetWidth(c->image)!=width || CGImageGetHeight(c->image)!=height) code=6;
                else {
                    size_t count=(size_t)width*height*4;
                    unsigned char *pixels=calloc(1,count);
                    CGColorSpaceRef space=CGColorSpaceCreateWithName(kCGColorSpaceSRGB);
                    CGContextRef bitmap=pixels && space?CGBitmapContextCreate(pixels,width,height,8,(size_t)width*4,space,kCGImageAlphaPremultipliedLast|kCGBitmapByteOrder32Big):NULL;
                    if(space) CGColorSpaceRelease(space);
                    if(!bitmap) {free(pixels);code=10;}
                    else {
                        // Same unflipped row convention as reviewed authentic raster.
                        CGContextSetBlendMode(bitmap,kCGBlendModeCopy);
                        CGContextDrawImage(bitmap,CGRectMake(0,0,width,height),c->image);
                        CGContextRelease(bitmap);
                        BOOL opaque=YES;
                        for(size_t i=3;i<count;i+=4) if(pixels[i]!=255) {opaque=NO;break;}
                        if(!opaque) {free(pixels);code=10;}
                        else {out->pixels=pixels;out->length=count;out->width=width;out->height=height;}
                    }
                }
            } }
        }
    }
    @synchronized(c.loader) {if(!code && (c.loader.refusals || !c.loader.delivered)) code=11;}
    [c retire];
    if(code && out->pixels) {free(out->pixels);out->pixels=NULL;out->length=0;}
    return code;
}
// The ONLY writes to the synchronous Rust output occur here/on the calling
// thread after owned result observation. out is never captured or retained.
void hair_still_decode(void *handle, const unsigned char *bytes, size_t length, HairStillResult *out) {
    if(!out) return;
    memset(out,0,sizeof(*out));
    @autoreleasepool {out->code=handle?Decode((__bridge HairStillSlot *)handle,bytes,length,out):3;}
    // A finite observation window only, not an SDK cancellation/deallocation
    // deadline. The caller keeps this SAME occupied slot if unobserved.
    if(handle) {
        double end=Now()+1.5;
        for(NSUInteger checks=0;checks<1500 && Now()<end;checks++) {
            BOOL available=NO;
            @autoreleasepool {available=Available((__bridge HairStillSlot *)handle);}
            if(available) break;
            struct timespec pause={0,1000000};nanosleep(&pause,NULL);
        }
    }
}
