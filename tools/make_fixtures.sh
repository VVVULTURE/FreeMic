#!/usr/bin/env bash
# Generates the audio fixtures used by tests/formats.rs (requires ffmpeg).
# Signal: 1.0 s, left = 440 Hz, right = 660 Hz, amplitude 0.5 (mono files: 440 Hz).
# (ffmpeg's sine source has amplitude 1/8, hence volume=4.)
set -euo pipefail
cd "$(dirname "$0")/../tests/fixtures"
F="ffmpeg -hide_banner -loglevel error -y"
ST="-filter_complex sine=f=440:r=48000:d=1,volume=4[l];sine=f=660:r=48000:d=1,volume=4[r];[l][r]join=inputs=2:channel_layout=stereo"
MONO="-f lavfi -i sine=f=440:r=48000:d=1,volume=4"
$F $ST -ar 44100 -c:a pcm_s16le stereo_s16_44k.wav
$F $ST -ar 96000 -c:a pcm_s24le stereo_s24_96k.wav
$F $ST -ar 48000 -c:a pcm_f32le stereo_f32_48k.wav
$F $MONO -ar 22050 -c:a pcm_u8 mono_u8_22k.wav
$F $MONO -ar 8000 -c:a pcm_s16le mono_s16_8k.wav
$F $ST -ar 44100 -c:a libmp3lame -b:a 192k stereo_44k.mp3
$F $MONO -ar 22050 -c:a libmp3lame -b:a 64k mono_22k.mp3
$F $ST -ar 48000 -c:a libvorbis -q:a 5 stereo_48k.ogg
$F $ST -ar 44100 -c:a flac -sample_fmt s32 -bits_per_raw_sample 24 stereo_s24_44k.flac
$F $MONO -ar 16000 -c:a flac mono_16k.flac
$F $ST -ar 44100 -c:a aac -b:a 160k stereo_aac_44k.m4a
$F $ST -ar 48000 -c:a alac stereo_alac_48k.m4a
# 5.1 surround (all channels 440 Hz) to exercise the downmix.
$F -f lavfi -i "sine=f=440:r=48000:d=1,volume=2.4" -af "pan=5.1|c0=c0|c1=c0|c2=c0|c3=c0|c4=c0|c5=c0" -c:a pcm_s16le surround51_48k.wav
# A long-ish file (20 s) for streaming / loop tests.
$F -f lavfi -i "sine=f=1000:r=44100:d=20,volume=4,pan=stereo|c0=c0|c1=c0" -c:a libmp3lame -b:a 128k long_20s.mp3
# Ogg Opus: not supported by the decoder; used to check the error message.
$F $MONO -c:a libopus opus_mono.ogg || true
# Broken inputs.
head -c 3000 stereo_44k.mp3 > truncated.mp3
head -c 64 stereo_f32_48k.wav > truncated_header.wav
: > empty.wav
printf 'this is not audio at all, just text pretending to be an mp3 file\n%.0s' {1..40} > not_audio.mp3
python3 - <<'PY' 2>/dev/null || python - <<'PY'
import random
random.seed(7)
d=bytearray(open('stereo_s24_44k.flac','rb').read())
for i in range(2000, len(d), 97):
    d[i]=random.randrange(256)
open('corrupt.flac','wb').write(bytes(d))
PY
import random
random.seed(7)
d=bytearray(open('stereo_s24_44k.flac','rb').read())
for i in range(2000, len(d), 97):
    d[i]=random.randrange(256)
open('corrupt.flac','wb').write(bytes(d))
PY
ls -la
