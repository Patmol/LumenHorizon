# Science Dataset Mapping Reference

This file records the evidence used by `processing-svc` for Chunk 8.2 HDF-EOS5 dataset lookup and quality filtering. Dataset names must be verified from NASA documentation and representative sample granules before implementation.

## Fixture Policy

Raw HDF-EOS5 granules are large and should not be committed unless they are explicitly reduced to small, license-safe fixtures.

Preferred evidence order:

1. NASA product documentation.
2. `gdalinfo` or equivalent subdataset enumeration from representative local sample granules.
3. Small derived metadata fixtures committed to the repository.

## GDAL Runtime Policy

Chunk 8.2 uses GDAL-capable tooling for HDF-EOS5 inspection, but `processing-svc` should not depend on the Rust `gdal` crate while current crate releases are incompatible with locally installed GDAL 3.13 bindings. The service should use the GDAL command-line boundary, starting with `gdalinfo`, and return clear runtime errors when the executable is unavailable.

Local developers must either install GDAL CLI tooling, for example with Homebrew on macOS, or use the future `processing-svc` container image. Container images must pin GDAL, PROJ, HDF5, and related native runtime packages so behavior does not depend on a developer workstation's GDAL version.

The known failure mode behind this policy is that generated GDAL 3.13 bindings expose renamed data type symbols such as `GDT_UInt8`, while current Rust `gdal` crate code still references older symbols such as `GDT_Byte`. Until the Rust bindings catch up, the CLI boundary is the stable project path.

## Products

### VNP46A2

| Field | Verified value | Evidence |
|-------|----------------|----------|
| Radiance dataset | `//HDFEOS/GRIDS/VIIRS_Grid_DNB_2d/Data_Fields/Gap_Filled_DNB_BRDF-Corrected_NTL` | `gdalinfo` metadata lines 36-43 and subdataset lines 164-165. |
| Mandatory quality dataset | `//HDFEOS/GRIDS/VIIRS_Grid_DNB_2d/Data_Fields/Mandatory_Quality_Flag` | `gdalinfo` metadata lines 64-77 and subdataset lines 168-169. |
| Cloud quality dataset | `//HDFEOS/GRIDS/VIIRS_Grid_DNB_2d/Data_Fields/QF_Cloud_Mask` | `gdalinfo` metadata lines 78-115 and subdataset lines 170-171. |
| Fill/nodata metadata | Radiance fill `-999.90002`, valid range `>=0.0`, scale `1`, offset `0`, units `nWatts/(cm^2 sr)`; mandatory quality fill `255`; cloud mask fill `65535`. | `gdalinfo` metadata lines 36-43, 72-77, and 110-115. |

Sample granule evidence:

```text
Local sample used for evidence: /Users/cedric/Downloads/h05v06.h5
131: LongName=VIIRS/NPP Gap-Filled Lunar BRDF-Adjusted Nighttime Lights Daily L3 Global 15 arc-second Linear Lat Lon Grid

Mapping decision:
Use Gap_Filled_DNB_BRDF-Corrected_NTL as the primary radiance dataset for VNP46A2 because this sample is the gap-filled daily product. DNB_BRDF-Corrected_NTL is also present in the file, but it is not selected as the primary product radiance layer for this mapping.

Radiance:
20: HDFEOS_GRIDS_VIIRS_Grid_DNB_2d_Data_Fields_DNB_BRDF-Corrected_NTL_long_name=BRDF Corrected DNB Radiance
27: HDFEOS_GRIDS_VIIRS_Grid_DNB_2d_Data_Fields_DNB_BRDF-Corrected_NTL__FillValue=-999.90002
36: HDFEOS_GRIDS_VIIRS_Grid_DNB_2d_Data_Fields_Gap_Filled_DNB_BRDF-Corrected_NTL_long_name=Gap Filled BRDF Corrected DNB Radiance
38: HDFEOS_GRIDS_VIIRS_Grid_DNB_2d_Data_Fields_Gap_Filled_DNB_BRDF-Corrected_NTL_scale_factor=1
39: HDFEOS_GRIDS_VIIRS_Grid_DNB_2d_Data_Fields_Gap_Filled_DNB_BRDF-Corrected_NTL_units=nWatts/(cm^2 sr)
41: HDFEOS_GRIDS_VIIRS_Grid_DNB_2d_Data_Fields_Gap_Filled_DNB_BRDF-Corrected_NTL_valid_range=>=0.0
43: HDFEOS_GRIDS_VIIRS_Grid_DNB_2d_Data_Fields_Gap_Filled_DNB_BRDF-Corrected_NTL__FillValue=-999.90002
160: SUBDATASET_1_NAME=HDF5:"/Users/cedric/Downloads/h05v06.h5"://HDFEOS/GRIDS/VIIRS_Grid_DNB_2d/Data_Fields/DNB_BRDF-Corrected_NTL
164: SUBDATASET_3_NAME=HDF5:"/Users/cedric/Downloads/h05v06.h5"://HDFEOS/GRIDS/VIIRS_Grid_DNB_2d/Data_Fields/Gap_Filled_DNB_BRDF-Corrected_NTL
165: SUBDATASET_3_DESC=[2400x2400] //HDFEOS/GRIDS/VIIRS_Grid_DNB_2d/Data_Fields/Gap_Filled_DNB_BRDF-Corrected_NTL (32-bit floating-point)

Mandatory quality:
64: HDFEOS_GRIDS_VIIRS_Grid_DNB_2d_Data_Fields_Mandatory_Quality_Flag_Description=00 High-Quality Main Algorithm
65: 01 Poor-Quality Main Algorithm (Outlier, Potential cloud contamination or other issues)
70: 255 No Retrieval Fill Value
72: HDFEOS_GRIDS_VIIRS_Grid_DNB_2d_Data_Fields_Mandatory_Quality_Flag_long_name=Mandatory Quality Flag of BRDF Corrected DNB Radiance
77: HDFEOS_GRIDS_VIIRS_Grid_DNB_2d_Data_Fields_Mandatory_Quality_Flag__FillValue=255
168: SUBDATASET_5_NAME=HDF5:"/Users/cedric/Downloads/h05v06.h5"://HDFEOS/GRIDS/VIIRS_Grid_DNB_2d/Data_Fields/Mandatory_Quality_Flag
169: SUBDATASET_5_DESC=[2400x2400] //HDFEOS/GRIDS/VIIRS_Grid_DNB_2d/Data_Fields/Mandatory_Quality_Flag (8-bit unsigned character)

Cloud quality:
78: HDFEOS_GRIDS_VIIRS_Grid_DNB_2d_Data_Fields_QF_Cloud_Mask_Description=bit Flag description key:
87: 4-5 Cloud Mask Quality
92: 6-7 Cloud Detection Results & Confidence Indicator
95: 10=Probably Cloudy
96: 11=Confident Cloudy
110: HDFEOS_GRIDS_VIIRS_Grid_DNB_2d_Data_Fields_QF_Cloud_Mask_long_name=Cloud Mask Status
115: HDFEOS_GRIDS_VIIRS_Grid_DNB_2d_Data_Fields_QF_Cloud_Mask__FillValue=65535
170: SUBDATASET_6_NAME=HDF5:"/Users/cedric/Downloads/h05v06.h5"://HDFEOS/GRIDS/VIIRS_Grid_DNB_2d/Data_Fields/QF_Cloud_Mask
171: SUBDATASET_6_DESC=[2400x2400] //HDFEOS/GRIDS/VIIRS_Grid_DNB_2d/Data_Fields/QF_Cloud_Mask (16-bit unsigned integer)
```

### VJ146A2

| Field | Verified value | Evidence |
|-------|----------------|----------|
| Radiance dataset | `//HDFEOS/GRIDS/VIIRS_Grid_DNB_2d/Data_Fields/Gap_Filled_DNB_BRDF-Corrected_NTL` | `gdalinfo` metadata lines 36-43 and subdataset lines 164-165. |
| Mandatory quality dataset | `//HDFEOS/GRIDS/VIIRS_Grid_DNB_2d/Data_Fields/Mandatory_Quality_Flag` | `gdalinfo` metadata lines 64-77 and subdataset lines 168-169. |
| Cloud quality dataset | `//HDFEOS/GRIDS/VIIRS_Grid_DNB_2d/Data_Fields/QF_Cloud_Mask` | `gdalinfo` metadata lines 78-115 and subdataset lines 170-171. |
| Fill/nodata metadata | Radiance fill `-999.90002`, valid range `>=0.0`, scale `1`, offset `0`, units `nWatts/(cm^2 sr)`; mandatory quality fill `255`; cloud mask fill `65535`. | `gdalinfo` metadata lines 36-43, 72-77, and 110-115. |

Sample granule evidence:

```text
Local sample used for evidence: /Users/cedric/Downloads/VJ146A2.A2018019.h01v01.002.2025283105950.h5
131: LongName=VIIRS/JPSS1 Gap-Filled Lunar BRDF-Adjusted Nighttime Lights Daily L3 Global 15 arc-second Linear Lat Lon Grid

Mapping decision:
Use Gap_Filled_DNB_BRDF-Corrected_NTL as the primary radiance dataset for VJ146A2 because this sample is the gap-filled daily JPSS1 product. DNB_BRDF-Corrected_NTL is also present in the file, but it is not selected as the primary product radiance layer for this mapping.

Radiance:
20: HDFEOS_GRIDS_VIIRS_Grid_DNB_2d_Data_Fields_DNB_BRDF-Corrected_NTL_long_name=BRDF Corrected DNB Radiance
27: HDFEOS_GRIDS_VIIRS_Grid_DNB_2d_Data_Fields_DNB_BRDF-Corrected_NTL__FillValue=-999.90002
36: HDFEOS_GRIDS_VIIRS_Grid_DNB_2d_Data_Fields_Gap_Filled_DNB_BRDF-Corrected_NTL_long_name=Gap Filled BRDF Corrected DNB Radiance
38: HDFEOS_GRIDS_VIIRS_Grid_DNB_2d_Data_Fields_Gap_Filled_DNB_BRDF-Corrected_NTL_scale_factor=1
39: HDFEOS_GRIDS_VIIRS_Grid_DNB_2d_Data_Fields_Gap_Filled_DNB_BRDF-Corrected_NTL_units=nWatts/(cm^2 sr)
41: HDFEOS_GRIDS_VIIRS_Grid_DNB_2d_Data_Fields_Gap_Filled_DNB_BRDF-Corrected_NTL_valid_range=>=0.0
43: HDFEOS_GRIDS_VIIRS_Grid_DNB_2d_Data_Fields_Gap_Filled_DNB_BRDF-Corrected_NTL__FillValue=-999.90002
160: SUBDATASET_1_NAME=HDF5:"/Users/cedric/Downloads/VJ146A2.A2018019.h01v01.002.2025283105950.h5"://HDFEOS/GRIDS/VIIRS_Grid_DNB_2d/Data_Fields/DNB_BRDF-Corrected_NTL
164: SUBDATASET_3_NAME=HDF5:"/Users/cedric/Downloads/VJ146A2.A2018019.h01v01.002.2025283105950.h5"://HDFEOS/GRIDS/VIIRS_Grid_DNB_2d/Data_Fields/Gap_Filled_DNB_BRDF-Corrected_NTL
165: SUBDATASET_3_DESC=[2400x2400] //HDFEOS/GRIDS/VIIRS_Grid_DNB_2d/Data_Fields/Gap_Filled_DNB_BRDF-Corrected_NTL (32-bit floating-point)

Mandatory quality:
64: HDFEOS_GRIDS_VIIRS_Grid_DNB_2d_Data_Fields_Mandatory_Quality_Flag_Description=00 High-Quality Main Algorithm
65: 01 Poor-Quality Main Algorithm (Outlier, Potential cloud contamination or other issues)
70: 255 No Retrieval Fill Value
72: HDFEOS_GRIDS_VIIRS_Grid_DNB_2d_Data_Fields_Mandatory_Quality_Flag_long_name=Mandatory Quality Flag of BRDF Corrected DNB Radiance
77: HDFEOS_GRIDS_VIIRS_Grid_DNB_2d_Data_Fields_Mandatory_Quality_Flag__FillValue=255
168: SUBDATASET_5_NAME=HDF5:"/Users/cedric/Downloads/VJ146A2.A2018019.h01v01.002.2025283105950.h5"://HDFEOS/GRIDS/VIIRS_Grid_DNB_2d/Data_Fields/Mandatory_Quality_Flag
169: SUBDATASET_5_DESC=[2400x2400] //HDFEOS/GRIDS/VIIRS_Grid_DNB_2d/Data_Fields/Mandatory_Quality_Flag (8-bit unsigned character)

Cloud quality:
78: HDFEOS_GRIDS_VIIRS_Grid_DNB_2d_Data_Fields_QF_Cloud_Mask_Description=bit Flag description key:
87: 4-5 Cloud Mask Quality
92: 6-7 Cloud Detection Results & Confidence Indicator
95: 10=Probably Cloudy
96: 11=Confident Cloudy
110: HDFEOS_GRIDS_VIIRS_Grid_DNB_2d_Data_Fields_QF_Cloud_Mask_long_name=Cloud Mask Status
115: HDFEOS_GRIDS_VIIRS_Grid_DNB_2d_Data_Fields_QF_Cloud_Mask__FillValue=65535
170: SUBDATASET_6_NAME=HDF5:"/Users/cedric/Downloads/VJ146A2.A2018019.h01v01.002.2025283105950.h5"://HDFEOS/GRIDS/VIIRS_Grid_DNB_2d/Data_Fields/QF_Cloud_Mask
171: SUBDATASET_6_DESC=[2400x2400] //HDFEOS/GRIDS/VIIRS_Grid_DNB_2d/Data_Fields/QF_Cloud_Mask (16-bit unsigned integer)

Chunk 8.3 will add job-backed daily ingestion after this mapping is implemented.
```

### VNP46A3

| Field | Verified value | Evidence |
|-------|----------------|----------|
| Radiance dataset | `//HDFEOS/GRIDS/VIIRS_Grid_DNB_2d/Data_Fields/AllAngle_Composite_Snow_Free` | `gdalinfo` metadata lines 56, 78, 87-90 and subdataset lines 319-320. |
| Quality dataset | `//HDFEOS/GRIDS/VIIRS_Grid_DNB_2d/Data_Fields/AllAngle_Composite_Snow_Free_Quality` | `gdalinfo` metadata lines 66-77 and subdataset lines 323-324. |
| Valid-observation dataset | `//HDFEOS/GRIDS/VIIRS_Grid_DNB_2d/Data_Fields/AllAngle_Composite_Snow_Free_Num` | `gdalinfo` metadata lines 57-63 and subdataset lines 321-322. |
| Fill/nodata metadata | Radiance fill `-999.90002`, valid range `>= 0.0`, scale `1`, offset `0`, units `nWatts/(cm^2 sr)`; quality fill `255`; observation-count fill `65535`. | `gdalinfo` metadata lines 59-63, 66-77, and 78-90. |

Sample granule evidence:

```text
Local sample used for evidence: /Users/cedric/Downloads/VNP46A3.A2012001.h27v03.002.2025086154548.h5
280: LongName=VIIRS/NPP Lunar BRDF-Adjusted Nighttime Lights Monthly L3 Global 15 arc second Linear Lat Lon Grid

Mapping decision:
Use AllAngle_Composite_Snow_Free as the initial primary monthly radiance dataset for VNP46A3 because this sample is a monthly product and this layer is the temporal radiance composite using all observations during the snow-free period. Its matching observation-count and quality datasets are AllAngle_Composite_Snow_Free_Num and AllAngle_Composite_Snow_Free_Quality.

Primary monthly radiance:
56: HDFEOS_GRIDS_VIIRS_Grid_DNB_2d_Data_Fields_AllAngle_Composite_Snow_Free_long_name=Temporal Radiance Composite Using All Observations During Snow-free Period
78: HDFEOS_GRIDS_VIIRS_Grid_DNB_2d_Data_Fields_AllAngle_Composite_Snow_Free_scale_factor=1
87: HDFEOS_GRIDS_VIIRS_Grid_DNB_2d_Data_Fields_AllAngle_Composite_Snow_Free_units=nWatts/(cm^2 sr)
88: HDFEOS_GRIDS_VIIRS_Grid_DNB_2d_Data_Fields_AllAngle_Composite_Snow_Free_valid_range=>= 0.0
90: HDFEOS_GRIDS_VIIRS_Grid_DNB_2d_Data_Fields_AllAngle_Composite_Snow_Free__FillValue=-999.90002
319: SUBDATASET_5_NAME=HDF5:"/Users/cedric/Downloads/VNP46A3.A2012001.h27v03.002.2025086154548.h5"://HDFEOS/GRIDS/VIIRS_Grid_DNB_2d/Data_Fields/AllAngle_Composite_Snow_Free
320: SUBDATASET_5_DESC=[2400x2400] //HDFEOS/GRIDS/VIIRS_Grid_DNB_2d/Data_Fields/AllAngle_Composite_Snow_Free (32-bit floating-point)

Observation count:
57: HDFEOS_GRIDS_VIIRS_Grid_DNB_2d_Data_Fields_AllAngle_Composite_Snow_Free_Num_coordinates=latitude longitude
58: HDFEOS_GRIDS_VIIRS_Grid_DNB_2d_Data_Fields_AllAngle_Composite_Snow_Free_Num_long_name=Number of Observations of Temporal Radiance Composite Using All Observations During Snow-free Period
59: HDFEOS_GRIDS_VIIRS_Grid_DNB_2d_Data_Fields_AllAngle_Composite_Snow_Free_Num_offset=0
60: HDFEOS_GRIDS_VIIRS_Grid_DNB_2d_Data_Fields_AllAngle_Composite_Snow_Free_Num_scale_factor=1
61: HDFEOS_GRIDS_VIIRS_Grid_DNB_2d_Data_Fields_AllAngle_Composite_Snow_Free_Num_units=number of observations
62: HDFEOS_GRIDS_VIIRS_Grid_DNB_2d_Data_Fields_AllAngle_Composite_Snow_Free_Num_valid_range=0 65534
63: HDFEOS_GRIDS_VIIRS_Grid_DNB_2d_Data_Fields_AllAngle_Composite_Snow_Free_Num__FillValue=65535
321: SUBDATASET_6_NAME=HDF5:"/Users/cedric/Downloads/VNP46A3.A2012001.h27v03.002.2025086154548.h5"://HDFEOS/GRIDS/VIIRS_Grid_DNB_2d/Data_Fields/AllAngle_Composite_Snow_Free_Num
322: SUBDATASET_6_DESC=[2400x2400] //HDFEOS/GRIDS/VIIRS_Grid_DNB_2d/Data_Fields/AllAngle_Composite_Snow_Free_Num (16-bit unsigned integer)

Quality:
66: HDFEOS_GRIDS_VIIRS_Grid_DNB_2d_Data_Fields_AllAngle_Composite_Snow_Free_Quality_Description=Quality:
67: 0 = Good quality
68: 1 = Poor quality
69: 2 = Gap filled
70: 255 = Fill value
72: HDFEOS_GRIDS_VIIRS_Grid_DNB_2d_Data_Fields_AllAngle_Composite_Snow_Free_Quality_long_name=Quality Flag of Temporal Radiance Composite Using All Observations During Snow-free Period
77: HDFEOS_GRIDS_VIIRS_Grid_DNB_2d_Data_Fields_AllAngle_Composite_Snow_Free_Quality__FillValue=255
323: SUBDATASET_7_NAME=HDF5:"/Users/cedric/Downloads/VNP46A3.A2012001.h27v03.002.2025086154548.h5"://HDFEOS/GRIDS/VIIRS_Grid_DNB_2d/Data_Fields/AllAngle_Composite_Snow_Free_Quality
324: SUBDATASET_7_DESC=[2400x2400] //HDFEOS/GRIDS/VIIRS_Grid_DNB_2d/Data_Fields/AllAngle_Composite_Snow_Free_Quality (8-bit unsigned character)

Additional verified monthly radiance composite families in the same sample:
311: SUBDATASET_1_NAME=HDF5:"/Users/cedric/Downloads/VNP46A3.A2012001.h27v03.002.2025086154548.h5"://HDFEOS/GRIDS/VIIRS_Grid_DNB_2d/Data_Fields/AllAngle_Composite_Snow_Covered
331: SUBDATASET_11_NAME=HDF5:"/Users/cedric/Downloads/VNP46A3.A2012001.h27v03.002.2025086154548.h5"://HDFEOS/GRIDS/VIIRS_Grid_DNB_2d/Data_Fields/NearNadir_Composite_Snow_Covered
339: SUBDATASET_15_NAME=HDF5:"/Users/cedric/Downloads/VNP46A3.A2012001.h27v03.002.2025086154548.h5"://HDFEOS/GRIDS/VIIRS_Grid_DNB_2d/Data_Fields/NearNadir_Composite_Snow_Free
347: SUBDATASET_19_NAME=HDF5:"/Users/cedric/Downloads/VNP46A3.A2012001.h27v03.002.2025086154548.h5"://HDFEOS/GRIDS/VIIRS_Grid_DNB_2d/Data_Fields/OffNadir_Composite_Snow_Covered
355: SUBDATASET_23_NAME=HDF5:"/Users/cedric/Downloads/VNP46A3.A2012001.h27v03.002.2025086154548.h5"://HDFEOS/GRIDS/VIIRS_Grid_DNB_2d/Data_Fields/OffNadir_Composite_Snow_Free

Chunk 8.3 will add job-backed monthly composite ingestion after this mapping is implemented.
```

## Open Decisions

- Whether raw sample granules live outside git with documented paths, or whether reduced metadata fixtures are committed under `backend/processing-svc/fixtures/`.
- Manual sample downloads are acceptable for Chunk 8.2 mapping evidence only; Chunk 8.3 owns job-backed ingest enablement for daily `VNP46A2`/`VJ146A2` products and monthly `VNP46A3` composites.
- Exact quality flags used for valid-pixel and cloud-contamination decisions.
- Whether `VNP46A3` monthly processing lands fully in Chunk 8.2 or is mapped now and processed after daily products.
