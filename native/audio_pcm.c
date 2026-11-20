/* Validated PCM ONLY. No file, URL, parser, input or microphone authority.
 * The callback owns only this C allocation, never Rust data.
 */
#include <AudioToolbox/AudioQueue.h>
#include <pthread.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
/* Low four bits are the reason; upper bits identify the current explicit
 * start/pause epoch. A listener's read/query/CAS cannot cross an owner transition.
 * No notification carries an Apple event sequence number: see handoff limits.
 */
enum { PREPARED, STARTING, RUNNING, DRAIN_REQUEST, DRAINING, DRAIN_SEEN,
       TERMINAL, PAUSED, CLOSING };
enum { HAIR_NO_OWNER, HAIR_QUEUE_OWNED, HAIR_CREATION_UNCERTAIN };
#define HAIR_QUARANTINED (-9)
#define REASON(v) ((v) & 15u)
typedef struct {
    AudioQueueRef queue; /* assigned ONLY after successful NewOutput */
    pthread_mutex_t control; /* callback only trylocks; listener never locks */
    _Atomic unsigned lifecycle;
    _Atomic int returned;
    _Atomic int error;
    _Atomic int retiring;
    int quarantined; /* owner-thread only */
} HairPCM;
static void running_changed(void *context, AudioQueueRef queue, AudioQueuePropertyID id) {
    HairPCM *p = context;
    if (id != kAudioQueueProperty_IsRunning || atomic_load(&p->retiring)) return;
    unsigned before = atomic_load(&p->lifecycle), after = before;
    UInt32 running = 0, size = sizeof(running);
    OSStatus s = AudioQueueGetProperty(queue, id, &running, &size);
    if (s || size != sizeof(running)) {
        atomic_store(&p->error, s ? s : -6);
        return;
    }
    if (running && REASON(before) == STARTING) after = before - STARTING + RUNNING;
    /* Only a listener event following an acknowledged start and an explicit
     * terminal Stop(false) can end this epoch. A false poll is never terminal.
     * DRAIN_SEEN records an inline event until Stop's success is confirmed.
     */
    if (!running && REASON(before) == DRAIN_REQUEST) after = before - DRAIN_REQUEST + DRAIN_SEEN;
    if (!running && REASON(before) == DRAINING) after = before - DRAINING + TERMINAL;
    if (after != before) {
        unsigned observed = before;
        if (!atomic_compare_exchange_strong(&p->lifecycle, &observed, after)
            && !running && REASON(before) == DRAIN_REQUEST
            && observed == before - DRAIN_REQUEST + DRAINING) {
            /* Stop succeeded in this exact epoch while the false query was
             * in flight. Preserve that listener observation, but nothing from
             * another epoch/reason; a concurrent owner transition still wins.
             */
            (void)atomic_compare_exchange_strong(&p->lifecycle, &observed,
                                                before - DRAIN_REQUEST + TERMINAL);
        }
    }
}
/* control mutex held, never by property listener. The callback may defer this
 * work to the next owner poll rather than race Pause/Start/Dispose.
 */
static void request_drain(HairPCM *p) {
    unsigned before = atomic_load(&p->lifecycle);
    if (REASON(before) != RUNNING || !atomic_load(&p->returned)) return;
    unsigned request = before - RUNNING + DRAIN_REQUEST;
    if (!atomic_compare_exchange_strong(&p->lifecycle, &before, request)) return;
    OSStatus s = AudioQueueStop(p->queue, false);
    if (s) { atomic_store(&p->error, s); return; }
    before = request;
    if (!atomic_compare_exchange_strong(&p->lifecycle, &before, request - DRAIN_REQUEST + DRAINING)) {
        before = request - DRAIN_REQUEST + DRAIN_SEEN;
        (void)atomic_compare_exchange_strong(&p->lifecycle, &before, request - DRAIN_REQUEST + TERMINAL);
    }
}
static void completed(void *context, AudioQueueRef queue, AudioQueueBufferRef buffer) {
    HairPCM *p = context;
    (void)queue; (void)buffer;
    if (atomic_load(&p->retiring)) return;
    /* Buffer reuse is not EOF and does not by itself authorize Stop or Start. */
    atomic_store(&p->returned, 1);
    if (pthread_mutex_trylock(&p->control) != 0) return;
    if (!atomic_load(&p->retiring)) request_drain(p);
    pthread_mutex_unlock(&p->control);
}
/* A creation-error context is known memory, NOT an owned queue. Never inspect
 * or call ANY queue API on the error out value. Both error+null and error+nonnull
 * quarantine context: the SDK gives no failure callback-quiescence guarantee.
 * Successful Dispose(true), outside callbacks, is the only queue-owner free.
 */
int32_t hair_pcm_close(HairPCM *p) {
    if (!p) return 0;
    if (p->quarantined) return HAIR_QUARANTINED;
    pthread_mutex_lock(&p->control);
    atomic_store(&p->retiring, 1);
    atomic_store(&p->lifecycle, CLOSING);
    if (!p->queue) {
        p->quarantined = 1;
        pthread_mutex_unlock(&p->control);
        return HAIR_QUARANTINED;
    }
    (void)AudioQueueStop(p->queue, true);
    OSStatus s = AudioQueueDispose(p->queue, true);
    if (s) {
        p->quarantined = 1;
        pthread_mutex_unlock(&p->control);
        return s;
    }
    p->queue = NULL;
    pthread_mutex_unlock(&p->control);
    pthread_mutex_destroy(&p->control);
    free(p);
    return 0;
}
/* ownership ABI: 0 no acquisition, 1 successful NewOutput owner (including
 * later-stage errors), 2 uncertain creation error: permanently poison Rust.
 * Status remains the original OSStatus. Error+null returns no Rust context;
 * its known C allocation is still deliberately quarantined, not freed.
 */
int32_t hair_pcm_open(const int16_t *samples, uint32_t count, uint32_t channels,
                      uint32_t rate, HairPCM **out, uint32_t *ownership) {
    if (!out || !ownership) return -1;
    *out = NULL; *ownership = HAIR_NO_OWNER;
    if (!samples || !count || count > 2097152 || (channels != 1 && channels != 2)
        || count % channels || rate < 8000 || rate > 96000) return -2;
    uint32_t bytes = count * 2;
    HairPCM *p = calloc(1, sizeof(*p));
    if (!p) return -3;
    if (pthread_mutex_init(&p->control, NULL)) { free(p); return -3; }
    atomic_init(&p->lifecycle, PREPARED);
    atomic_init(&p->returned, 0); atomic_init(&p->error, 0); atomic_init(&p->retiring, 0);
    AudioStreamBasicDescription f = {0};
    f.mSampleRate = rate; f.mFormatID = kAudioFormatLinearPCM;
    f.mFormatFlags = kAudioFormatFlagIsSignedInteger | kAudioFormatFlagIsPacked;
#if __BIG_ENDIAN__
    f.mFormatFlags |= kAudioFormatFlagIsBigEndian;
#endif
    f.mBytesPerPacket = channels * 2; f.mFramesPerPacket = 1;
    f.mBytesPerFrame = channels * 2; f.mChannelsPerFrame = channels; f.mBitsPerChannel = 16;
    AudioQueueRef candidate = NULL;
    OSStatus s = AudioQueueNewOutput(&f, completed, p, NULL, NULL, 0, &candidate);
    if (s || !candidate) {
        atomic_store(&p->retiring, 1);
        p->quarantined = 1;
        *ownership = HAIR_CREATION_UNCERTAIN;
        if (candidate) *out = p; /* known context, never candidate ownership */
        return s ? s : -8;
    }
    p->queue = candidate;
    *ownership = HAIR_QUEUE_OWNED;
    *out = p;
    s = AudioQueueAddPropertyListener(p->queue, kAudioQueueProperty_IsRunning, running_changed, p);
    if (s) return s;
    /* Listener lifetime ends with successful Dispose, not Remove alone. */
    AudioQueueBufferRef b = NULL;
    s = AudioQueueAllocateBuffer(p->queue, bytes, &b);
    if (s) return s;
    if (!b || !b->mAudioData || b->mAudioDataBytesCapacity < bytes) return -4;
    memcpy(b->mAudioData, samples, bytes); b->mAudioDataByteSize = bytes;
    s = AudioQueueEnqueueBuffer(p->queue, b, 0, NULL);
    if (s) return s;
    return AudioQueueSetParameter(p->queue, kAudioQueueParam_Volume, 0.10f);
}
/* Queue-local gain only. Comparisons reject NaN, infinities and out-of-range
 * values before any AudioQueue call. Never Start/Prime/Enqueue from gain control.
 * Owner serialization is the same control mutex used by play/pause/close.
 */
int32_t hair_pcm_gain(HairPCM *p, float gain) {
    if (!(gain >= 0.0f && gain <= 1.0f)) return -2;
    if (!p || p->quarantined || atomic_load(&p->retiring) || !p->queue) return -7;
    pthread_mutex_lock(&p->control);
    OSStatus s = AudioQueueSetParameter(p->queue, kAudioQueueParam_Volume, gain);
    if (s) atomic_store(&p->error, s);
    pthread_mutex_unlock(&p->control);
    return s;
}
int32_t hair_pcm_play(HairPCM *p) {
    if (!p || p->quarantined || atomic_load(&p->retiring) || !p->queue) return -7;
    pthread_mutex_lock(&p->control);
    unsigned before = atomic_load(&p->lifecycle);
    if (REASON(before) == TERMINAL) { pthread_mutex_unlock(&p->control); return 0; }
    atomic_store(&p->lifecycle, ((before & ~15u) + 16u) | STARTING);
    OSStatus s = AudioQueueStart(p->queue, NULL);
    if (s) atomic_store(&p->error, s);
    pthread_mutex_unlock(&p->control);
    return s;
}
int32_t hair_pcm_pause(HairPCM *p) {
    if (!p || p->quarantined || atomic_load(&p->retiring) || !p->queue) return -7;
    pthread_mutex_lock(&p->control);
    unsigned before = atomic_load(&p->lifecycle);
    if (REASON(before) == TERMINAL) { pthread_mutex_unlock(&p->control); return 0; }
    /* Invalidate drain/start acknowledgements BEFORE entering the Pause API. */
    atomic_store(&p->lifecycle, ((before & ~15u) + 16u) | PAUSED);
    OSStatus s = AudioQueuePause(p->queue);
    if (s) atomic_store(&p->error, s);
    pthread_mutex_unlock(&p->control);
    return s;
}
/* 0 not running/pending, 1 running, 2 qualified terminal event. */
int32_t hair_pcm_state(HairPCM *p, uint32_t *state) {
    if (!p || !state || p->quarantined || atomic_load(&p->retiring) || !p->queue) return -5;
    pthread_mutex_lock(&p->control);
    UInt32 running = 0, size = sizeof(running);
    OSStatus s = AudioQueueGetProperty(p->queue, kAudioQueueProperty_IsRunning, &running, &size);
    if (!s && size != sizeof(running)) s = -6;
    if (!s) {
        unsigned before = atomic_load(&p->lifecycle);
        if (running && REASON(before) == STARTING)
            (void)atomic_compare_exchange_strong(&p->lifecycle, &before, before - STARTING + RUNNING);
        request_drain(p);
        unsigned reason = REASON(atomic_load(&p->lifecycle));
        *state = reason == TERMINAL ? 2 : (reason == STARTING ? 0 : (running ? 1 : 0));
        s = atomic_load(&p->error);
    }
    pthread_mutex_unlock(&p->control);
    return s;
}
