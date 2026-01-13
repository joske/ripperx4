# ripperX4 Feature Comparison

## ripperX4 vs C++ ripperX (ripperX3)

| Feature                  | C++ ripperX                                   | ripperX4                    |
| ------------------------ | --------------------------------------------- | --------------------------- |
| GUI toolkit              | GTK3                                          | GTK4                        |
| Encoders                 | MP3, OGG, FLAC, MP2, Musepack, Opus (plugins) | MP3, OGG, FLAC, Opus, WAV   |
| Multiple encoders        | Yes (encode to multiple formats at once)      | No (single format)          |
| External encoder support | Yes (plugins for lame, oggenc, flac, etc.)    | No (GStreamer built-in)     |
| Ripper                   | cdparanoia (configurable)                     | GStreamer cdparanoiasrc     |
| Metadata source          | GNUDB/CDDB only                               | CD-Text, MusicBrainz, GNUDB |
| File naming patterns     | Yes (%a - %s, %v, etc.)                       | Yes                         |
| Dir naming patterns      | Yes (%a - %v format)                          | No (fixed Artist-Album)     |
| Eject when done          | Yes                                           | Yes                         |
| Playlist (M3U)           | Yes                                           | Yes                         |
| Keep WAV files           | Yes                                           | No                          |
| Overwrite warning        | Yes (configurable)                            | Yes                         |
| Convert spaces           | Yes (configurable)                            | No                          |
| CD playback              | Yes (external player)                         | No                          |
| WAV/MP3 playback         | Yes (external player)                         | No                          |
| Per-encoder quality      | Yes (bitrate, VBR, quality per encoder)       | Low/Medium/High global      |
| Extra encoder options    | Yes (custom CLI flags)                        | No                          |
| Proxy support            | Yes (for CDDB)                                | No                          |
| ID3 tags                 | Yes                                           | Yes                         |
| Split "Artist - Title"   | Yes                                           | No                          |
| Auto lookup              | Yes (configurable)                            | Yes (always on)             |

Missing in ripperX4 vs C++ ripperX:

1. Configurable file/directory naming patterns
2. Multiple simultaneous encoders
3. Keep WAV files option
4. CD/WAV/MP3 playback
5. Per-encoder quality settings
6. Extra encoder options

ripperX4 advantages over C++ ripperX:

- CD-Text support (local metadata, no network needed)
- MusicBrainz lookup (more accurate than GNUDB)
- Modern GTK4 UI
- No external encoder dependencies (uses GStreamer)
- Simpler configuration

## ripperX4 vs Sound Juicer

| Feature               | Sound Juicer                                      | ripperX4                     |
| --------------------- | ------------------------------------------------- | ---------------------------- |
| Encoders              | Any GStreamer profile (MP3, OGG, FLAC, AAC, etc.) | MP3, OGG, FLAC, Opus, WAV    |
| Metadata source       | MusicBrainz, CD-Text, FreeDB                      | CD-Text, MusicBrainz, GNUDB  |
| Multiple matches      | Yes (album chooser dialog)                        | No (takes first match)       |
| File naming patterns  | Yes (14 path + 10 file patterns)                  | Yes                          |
| Eject when done       | Yes                                               | Yes                          |
| Open folder when done | Yes                                               | No                           |
| Strip special chars   | Yes (configurable)                                | Yes (always on)              |
| CD drive selection    | Yes (multiple drives)                             | No (default drive only)      |
| Quality settings      | Via GStreamer profiles                            | Low/Medium/High              |
| Playlist (M3U)        | No                                                | Yes                          |
| Disc number           | Yes                                               | No                           |
| Composer              | Yes (full support)                                | Partial (data model, not UI) |
| Artist sortname       | Yes                                               | No                           |
| CD playback           | Yes                                               | No                           |
| Paranoia mode         | Yes (configurable)                                | No (GStreamer default)       |
| Overwrite warning     | No                                                | Yes                          |

Missing in ripperX4 vs Sound Juicer:

1. Configurable file naming patterns - biggest gap
2. Multiple MusicBrainz match selection
3. Open folder when done
4. CD drive selection (for systems with multiple drives)
5. CD playback

ripperX4 advantages over Sound Juicer:

- Overwrite warning dialog
- M3U playlist generation
- GNUDB fallback when MusicBrainz fails
- Simpler, more focused UI
