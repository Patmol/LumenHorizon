# Investigation: Remaining US Coverage Gaps After Mosaic Fix (West Coast + Central Strip)

## Status

Closed-with-findings; durable pipeline fix implemented in this branch. This is a
follow-up to `plans/investigation/latest-overlay-us-coverage-mosaic.md`. Root
cause for the remaining gaps is identified with live runtime evidence. The code
now removes ingest truncation, filters out-of-bounds discovery candidates before
download/enqueue, records VIIRS h/v coverage metadata on mosaics, and blocks
incomplete public-latest promotion unless explicitly overridden. The local
`2026-05-01` dataset still needs a complete re-ingest/reprocess/republish to
fill the existing holes.

## Context: What Already Got Better

The prior investigation found that `latest` pointed at a single most-recently
processed granule, so the overlay only showed one small tile. That has been
fixed: the backend now publishes per-granule intermediates as
`tile_set_kind='granule'` and publishes an explicit
product/date **mosaic** as public latest.

Confirmed live on 2026-06-23 via `http://127.0.0.1:8080/api/v1/tiles/manifest`:

- `tile_set_id`: `2026-05-01-radiance-dark-sky-v1-vnp46a3-mosaic-660ed55a`
- `tile_set_kind`: `mosaic`, `latest=true`
- `source_granules.length`: `8`
- `tile_count`: `9327`
- `bounds`: west `-120.234375`, south `23.885837699862`, east `-65.7421875`,
  north `50.06419173665909`

So the mosaic is real and now stitches multiple granules. That is why more of
the US is visible than before.

## Observed Symptom

Even with the working mosaic, the overlay still has two visible holes:

1. **The far West Coast** is missing (the Pacific seaboard west of roughly
   `-120.2` longitude: San Francisco, Portland, Seattle, coastal OR/WA/N. CA).
2. **A central vertical strip, north to south**, is missing (the Great Plains
   column running from the Canadian border down to the Gulf).

The latitude coverage is otherwise complete (~24 N to ~50 N).

## Root Cause Summary

The mosaic can only be as complete as the set of **ingested + processed**
source granules for that product/date. For `VNP46A3` `2026-05-01`, that source
set is missing two entire VIIRS tile **columns**:

- `h05` (longitude `-130 .. -120`) -> the West Coast strip inside the configured
  bounds (`-125 .. -120`).
- `h08` (longitude `-100 .. -90`) -> the central north-to-south strip.

These columns were never ingested, so they were never processed, so they are
absent from the mosaic. The visible holes match these two missing columns
exactly.

The primary reason they were never ingested is the local discovery cap
`INGEST_MAX_GRANULES=10`, which is smaller than the number of VIIRS `h/v` tiles
needed to cover the configured CONUS bounding box. A secondary factor is that
out-of-bounds Canadian tiles consume cap slots before they are rejected.

## Evidence

### 1. The mosaic stitches 8 granule tile sets (the 8 surviving granules)

From `GET /api/v1/tiles/sets?limit=40`, the `2026-05-01` granule tile sets that
feed the mosaic (kind=`granule`, latest=`false`) have these bounds:

| tile_count | west     | east     | south  | north  | column (lon)    | row (lat) |
| ---------- | -------- | -------- | ------ | ------ | --------------- | --------- |
| 1644       | -110.039 | -99.844  | 39.910 | 50.064 | h07 (-110/-100) | v04       |
| 1630       | -90.000  | -79.805  | 39.910 | 50.064 | h09 (-90/-80)   | v04       |
| 1449       | -110.039 | -99.844  | 29.841 | 40.179 | h07 (-110/-100) | v05       |
| 1439       | -90.000  | -79.805  | 29.841 | 40.179 | h09 (-90/-80)   | v05       |
| 1449       | -80.156  | -69.961  | 29.841 | 40.179 | h10 (-80/-70)   | v05       |
| 823        | -120.234 | -109.688 | 23.886 | 30.145 | h06 (-120/-110) | v06       |
| 801        | -90.000  | -79.805  | 23.886 | 30.145 | h09 (-90/-80)   | v06       |
| 371        | -70.313  | -65.742  | 23.886 | 30.145 | h11 (-70/-60)   | v06       |

### 2. Database confirms what was ingested vs processed vs rejected

Local Postgres (`localhost:5432/lumenhorizon`):

```text
ingest_log    VNP46A3 2026-05-01 enqueued   = 10
processing_log VNP46A3 2026-05-01 processed = 8
processing_log VNP46A3 2026-05-01 rejected  = 2
```

Per-tile ingest (all 10 enqueued granules):

```text
h/v ingested: (6,3) (6,6) (7,3) (7,4) (7,5) (9,4) (9,5) (9,6) (10,5) (11,6)
```

Per-tile processing outcome:

```text
processed: (6,6) (7,4) (7,5) (9,4) (9,5) (9,6) (10,5) (11,6)
rejected : (6,3) (7,3)
```

The two rejections are correct out-of-bounds rejections (full error message):

```text
h6v3: source bounds west:-120 south:50 east:-110 north:60
      do not overlap configured tile bounds west:-125 south:24 east:-66 north:50
h7v3: source bounds west:-110 south:50 east:-100 north:60
      do not overlap configured tile bounds west:-125 south:24 east:-66 north:50
```

`v03` spans latitude `50..60` (Canada). The configured north bound is `50`, so
those granules are legitimately rejected.

### 3. VIIRS tile -> bounds mapping (authoritative)

`backend/shared/src/slippy_tiles.rs::viirs_tile_bounds`:

```text
west  = -180 + tile_h * 10
east  = west + 10
north =   90 - tile_v * 10
south = north - 10
```

Column (longitude) mapping:

| tile_h | longitude span | notes                                    |
| ------ | -------------- | ---------------------------------------- |
| h05    | -130 .. -120   | West Coast (clipped to -125 by bounds)   |
| h06    | -120 .. -110   | ingested                                 |
| h07    | -110 .. -100   | ingested                                 |
| **h08**| **-100 .. -90**| **MISSING -> central N-S gap**           |
| h09    | -90 .. -80     | ingested                                 |
| h10    | -80 .. -70     | ingested                                 |
| h11    | -70 .. -60     | ingested                                 |

Row (latitude) mapping:

| tile_v | latitude span | notes                                |
| ------ | ------------- | ------------------------------------ |
| v03    | 50 .. 60      | Canada, correctly rejected           |
| v04    | 40 .. 50      | ingested where columns exist         |
| v05    | 30 .. 40      | ingested where columns exist         |
| v06    | 24 .. 30      | ingested where columns exist         |

This mapping matches the rejection messages and the granule tile-set bounds
above exactly.

### 4. Coverage grid (the holes are visible in the data)

`#` = present in mosaic, `R` = rejected (Canada), blank = never ingested.

```text
              h05        h06        h07        h08        h09        h10        h11
           -130/-120  -120/-110  -110/-100  -100/-90   -90/-80    -80/-70    -70/-60
v04 40-50                            #                     #
v05 30-40                            #                     #          #
v06 24-30               #                                  #                     #
v03 50-60                 R          R                                            (Canada)
```

- **Column `h05`** (West Coast) is entirely empty -> matches "the West Coast is
  missing." It also explains why the mosaic's western bound is `-120.234` (the
  westernmost ingested tile is `h06`), instead of the configured `-125`.
- **Column `h08`** (central) is entirely empty -> matches "the center, from
  North to South, is missing."

## Why The Columns Are Missing

### Primary: the discovery cap is smaller than CONUS

- `.env` sets `INGEST_MAX_GRANULES=10` (line 72).
- `backend/ingest-svc/src/jobs.rs` applies the cap:
  ```rust
  let granules = discovery
      .products
      .iter()
      .flat_map(|product| product.granules.iter())
      .take(config.ingest_max_granules.unwrap_or(usize::MAX));
  ```
- The configured bounding box `TILE_BOUNDS=-125,24,-66,50` intersects VIIRS
  columns `h05..h11` (7 columns) across rows `v04..v06` (3 rows), i.e. on the
  order of ~18-21 candidate land tiles. A cap of `10` cannot cover that, so
  whole columns fall off the end of the discovery list and are never ingested.
- Exactly `10` rows were enqueued, equal to the cap, which is the fingerprint of
  a truncated discovery rather than an exhausted one.

Geometric certainty: a spatial query over `-125 .. -66` longitude **must**
return the `h08` (`-100 .. -90`) tiles, since that column is wholly inside the
box. So `h08` was available from discovery and dropped by the cap, not absent at
the source. `h05` overlaps the western edge (`-125 .. -120`) and is likewise
returned by the spatial query.

### Secondary: out-of-bounds tiles waste cap slots

- CMR discovery (`backend/ingest-svc/src/cmr.rs`) pages purely on the configured
  bounding box and does not pre-filter tiles whose usable area falls north of
  the configured `north=50` bound.
- The box's top edge at lat `50` clips into row `v03` (lat `50..60`), so Canadian
  `h06v03` and `h07v03` tiles are discovered, ingested, and counted against the
  cap, then rejected at processing.
- Result: 2 of the 10 cap slots were spent on tiles that produce no US coverage,
  squeezing useful in-bounds tiles (like `h05`/`h08`) further out of reach.

## Not These Causes (Ruled Out)

- **Not the previously-fixed per-granule latest bug.** Latest is a real mosaic
  with `source_granules.length = 8` and `tile_set_kind='mosaic'`.
- **Not a processing rejection of the missing columns.** `h05` and `h08` have no
  rows at all in `ingest_log`/`processing_log`; they were never ingested. The
  only rejections are the correct Canadian `v03` tiles.
- **Not over-aggressive cloud/quality filtering.** Processed granules show
  `cloud_fraction=0`; nothing valid in `h05`/`h08` was discarded because nothing
  in `h05`/`h08` was ever fetched.
- **Not a mosaic bounds/derivation bug.** The mosaic bounds (`west=-120.234`)
  correctly reflect the union of the 8 ingested granules.
- **Not a missing-blob/publication mismatch.** The 8 included granule tile sets
  exist and render; the holes are upstream at ingest, not at publish.

## Recommended Fix Directions

Pick based on whether the goal is "make this local dataset whole" or "make the
pipeline guarantee CONUS completeness."

### A. Make the local dataset whole (fastest)

1. Raise or remove the cap for a complete run, e.g. `INGEST_MAX_GRANULES=32`
   (or unset it) so all CONUS `h/v` tiles for the date are ingested.
2. Re-run ingest for the target product/date. **Gotcha:** ingest resumes from
   the last non-failed `ingest_log` date via
   `get_discovery_resume_points_for_products`, so a naive re-run may skip
   `2026-05-01`. The missing-tile rows for that date must actually be
   (re)discovered - verify the resume point does not jump past it.
3. Re-run processing to drain the new queue messages (`just processing`).
4. **Republish the mosaic** - the existing mosaic will not change on its own:
   `processing-svc publish-mosaic VNP46A3 2026-05-01 --public-latest`
   (or `just publish-mosaic VNP46A3 2026-05-01 true`).
5. Verify the new mosaic `bounds.west` reaches ~`-125` and
   `source_granules.length` grows to the full in-bounds column/row count.

### B. Make the pipeline robust (durable fix)

Implementation status: addressed in code on 2026-06-24. The fix removes the
flat `INGEST_MAX_GRANULES` cap instead of making it coverage-aware, adds shared
VIIRS h/v coverage helpers, filters out-of-bounds CMR candidates in ingest, adds
manifest `coverage` metadata for mosaics, and gates `--public-latest` promotion
when expected in-bounds tiles are missing. `--allow-incomplete-public-latest`
exists as an explicit operational override.

1. **Filter out-of-bounds tiles at discovery.** Drop candidates whose
  `viirs_tile_bounds` do not overlap the configured bounds before any download,
  ingest-log insert, or queue enqueue.
2. **Stop capping below coverage in dev, or cap per-axis.** A flat
  `take(10)` silently truncates coverage. The implementation removes the cap;
  use a narrower `BOUNDING_BOX` for small smoke runs instead.
3. **Add a coverage report.** Compare the expected `h/v` tile set for
  configured bounds against discovered/processed h/v records for a product/date,
  and surface missing columns/rows so a partial mosaic is never silently
  promoted as "US latest."
4. **Optionally gate mosaic promotion on completeness** (or annotate the
  manifest with a coverage percentage) so an incomplete mosaic is not presented
  as full US coverage. This is implemented for public-latest promotion with an
  explicit override flag.

## Verification / Repro Commands

```bash
# Mosaic shape and bounds
curl -s http://127.0.0.1:8080/api/v1/tiles/manifest \
  | jq '.data | {tile_set_id,tile_set_kind,tile_count,bounds,
                 source_granules: (.source_granules|length)}'

# Granule tile-set bounds that feed the mosaic
curl -s 'http://127.0.0.1:8080/api/v1/tiles/sets?limit=40' \
  | jq -c '.data[] | {kind:.tile_set_kind,latest,dataset_date,tile_count,bounds}'
```

```sql
-- Which h/v were ingested for the mosaic's product/date?
SELECT tile_h, tile_v, status
FROM ingest_log
WHERE product='VNP46A3' AND granule_date::date='2026-05-01'
ORDER BY tile_h, tile_v;

-- Processing outcomes and rejection reasons
SELECT tile_h, tile_v, status, cloud_fraction, error_message
FROM processing_log
WHERE product='VNP46A3' AND granule_date::date='2026-05-01'
ORDER BY tile_h, tile_v;
```

Expected after a complete re-ingest + reprocess + republish:

- `ingest_log` contains `h05..h11` x `v04..v06` (Canada `v03` may still appear
  and be rejected, or be filtered at discovery if Fix B.1 is applied).
- Mosaic `bounds.west` reaches ~`-125`, with no internal `h08` column gap.
- `source_granules.length` reflects all in-bounds columns/rows, not `8`.

## Acceptance Criteria

1. The missing West Coast (`h05`) and central (`h08`) columns are explained by
   absent `ingest_log`/`processing_log` rows, not by rejection or filtering.
   (Met by evidence above.)
2. A complete run ingests every in-bounds `h/v` tile for the target
   product/date, and the republished mosaic shows no full-column holes.
3. The discovery cap no longer silently truncates required coverage (raised,
   removed, or made coverage-aware), and out-of-bounds tiles no longer consume
   cap slots.
4. A repeatable check (coverage report or the SQL above) can confirm expected
   vs actual `h/v` coverage before a mosaic is promoted to public latest.

## Key Files

- `backend/ingest-svc/src/jobs.rs` - CMR discovery filtering, cap removal,
  expected-vs-discovered h/v coverage logging, and queue enqueue flow.
- `backend/ingest-svc/src/cmr.rs` - bounding-box paged discovery from CMR.
- `backend/shared/src/slippy_tiles.rs` - `viirs_tile_bounds` (`h/v` -> lon/lat)
  plus reusable VIIRS expected-grid and coverage-summary helpers.
- `backend/processing-svc/src/mosaic.rs` - mosaic source selection, coverage
  metadata, public-latest completeness gate, and publication.
- `backend/processing-svc/src/db.rs` - `select_mosaic_sources` (only `processed`
  `granule` tile sets for the product/date).
- `.env` - local `TILE_BOUNDS=-125,24,-66,50`; any stale
  `INGEST_MAX_GRANULES` value is ignored by current source code.
