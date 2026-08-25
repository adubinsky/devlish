# Energy Community Validation

## The Problem

Section 48E of the Internal Revenue Code provides a 10% bonus tax credit
for energy projects located in designated "energy communities." For a
portfolio of 39 battery storage projects worth $72.6M in eligible basis,
the EC bonus can add $4M+ in tax credits, but only if the claim is valid.

The DOE/NETL provides a public mapping tool that shows which areas qualify.
The IRS explicitly states this tool "may not be relied upon by taxpayers to
substantiate a tax return position." The legally controlling sources are
the IRS Notice appendices (XLSX files listing qualifying counties and
census tracts), which are updated annually as unemployment data changes.

**In practice, the DOE map can be wrong.** During Solar4K Set 01 analysis
(June 2026), the DOE layer showed 21 projects as qualifying under the
Statistical Area category. Cross-referencing against the actual IRS Notice
2026-39 Appendix 1 (CY2025 qualifying counties) revealed that none of
those 21 counties appeared in the list. The DFW metro area had qualified
in earlier years when unemployment exceeded the national average, but had
since dropped below the threshold. The DOE layer was stale.

This tool automates the multi-source cross-check that caught that error.

## What It Does

Validates project locations against four authoritative data sources,
in order of legal authority:

1. **IRS Notice appendices** (controlling): XLSX files listing qualifying
   counties (Statistical Area) and census tracts (Coal Closure)
2. **DOE/NETL ArcGIS layers** (informational): spatial queries against the
   Coal Closure and Statistical Area feature services
3. **US Census Bureau Geocoder**: address/coordinate to census tract GEOID,
   county FIPS code, and state
4. **Cross-check**: compares DOE results against IRS appendix data and
   flags discrepancies

## Usage

### Devlish Programs

```bash
# Full EC eligibility check for a location
devlish run lib/ec_check.dvl --input '{"latitude": 31.741180, "longitude": -96.372570}'

# Census geocode only
devlish run lib/census_geocoder.dvl --input '{"latitude": 31.741180, "longitude": -96.372570}'

# DOE Coal Closure check
devlish run lib/doe_coal_closure.dvl --input '{"latitude": 37.78, "longitude": -81.18}'

# DOE Statistical Area check
devlish run lib/doe_stat_area.dvl --input '{"latitude": 32.78, "longitude": -96.80}'

# IRS appendix lookup
devlish run lib/irs_appendix.dvl --input '{"appendix_url": "https://www.irs.gov/pub/irs-drop/n-26-39-appendix-1.xlsx", "notice": "2026-39", "lookup_fips": "48121"}'

# Batch validation
devlish run ec_validation.dvl --input input.json
```

The `.dvl` programs express the validation logic in readable English,
suitable for review by attorneys and underwriters.

## Input Format

```json
{
  "appendix_cy2025_url": "https://www.irs.gov/pub/irs-drop/n-26-39-appendix-1.xlsx",
  "appendix_cy2024_url": "https://www.irs.gov/pub/irs-drop/n-25-31-appendix-3.xlsx",
  "assertion_report_path": "ec_validation_report.json",
  "projects": [
    {
      "id": "6127",
      "address": "273 FM 1366, Wortham, TX 76693",
      "latitude": 31.741180,
      "longitude": -96.372570,
      "claimed_ec": true,
      "claimed_category": "Coal Closure",
      "basis": 1950000
    }
  ]
}
```

## Output

A JSON report with per-project results:

- Census tract GEOID, county FIPS, state
- DOE Coal Closure layer hit (yes/no, with closure details)
- DOE Statistical Area layer hit (yes/no, with MSA name)
- IRS appendix cross-check (found/not found in controlling list)

## Data Source Hierarchy

| Tier | Source | Authority |
|------|--------|-----------|
| 1 | IRS Notice appendices (2023-29 through 2026-39) | Legally controlling |
| 2 | Treasury consolidated datasets (home.treasury.gov) | Machine-readable IRS data |
| 3 | DOE/NETL ArcGIS layers (arcgis.netl.doe.gov) | Informational only |
| 4 | Census Bureau, BLS LAUS, MSHA, EIA | Upstream corroboration |

## Key Insight

Statistical Area eligibility changes annually when new unemployment data
is released. Coal Closure tracts are cumulative and never lose eligibility.
Always verify Statistical Area claims against the IRS appendix for the
calendar year of construction start or placed-in-service, not just the DOE
map.

## Architecture

Written as Devlish (.dvl) programs using generic language features:
- `Get the url at` for Census Bureau and DOE/NETL API calls
- `Download the url at ... to` for IRS appendix XLSX downloads
- `Read XLSX rows from` for appendix data extraction
- `Respond with` for structured JSON output

Library programs in `lib/`:
- `census_geocoder.dvl`: US Census Bureau geocoding API
- `doe_coal_closure.dvl`: DOE/NETL Coal Closure ArcGIS query
- `doe_stat_area.dvl`: DOE/NETL Statistical Area ArcGIS query
- `irs_appendix.dvl`: IRS appendix XLSX download, parse, and FIPS lookup
- `ec_check.dvl`: Combined eligibility check using all four sources
