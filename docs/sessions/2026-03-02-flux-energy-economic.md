# Session: flux-energy and flux-economic Data Sources
**Date:** 2026-03-02
**Status:** Complete ✅

## Task
Create two new data source scripts following the flux-earthquakes pattern:
- `flux-energy` — EIA API (WTI crude, Henry Hub, US electricity demand)
- `flux-economic` — FRED API (ISM PMI ×2, unemployment, CPI, GDP growth)

## Files Created

### flux-energy
- `/home/etl/flux-energy/energy.py` — main script
- `/home/etl/flux-energy/flux-energy.service` — systemd unit
- `/home/etl/flux-energy/.env` — template (fill in real values)

### flux-economic
- `/home/etl/flux-economic/economic.py` — main script
- `/home/etl/flux-economic/flux-economic.service` — systemd unit
- `/home/etl/flux-economic/.env` — template (fill in real values)

## Design Notes

**Both scripts follow earthquakes.py exactly:**
- `provision_namespace()` — provisions on first run, uses FLUX_NAMESPACE_TOKEN if set
- `publish_entity()` — POSTs to `/api/events` with Bearer token
- `fetch_and_publish()` — fetches each series, publishes; errors per-series don't abort loop
- `main()` — provision → poll loop with exponential backoff

**EIA API v2:**
- Endpoint: `https://api.eia.gov/v2/{category}/data/`
- Auth: `api_key` query param
- Pagination: `sort[0][column]=period, sort[0][direction]=desc, length=1`
- Facet filtering: `facets[series][]=RWTC` style params
- Response: `response.data[0]` contains `value`, `units`, `period`
- Series used: RWTC (WTI), RNGWHHD (Henry Hub), US48-D (electricity demand)

**FRED API:**
- Endpoint: `https://api.stlouisfed.org/fred/series/observations`
- Auth: `api_key` query param
- Latest only: `sort_order=desc, limit=1, file_type=json`
- Response: `observations[0]` contains `date`, `value`
- FRED uses `"."` for missing values — handled, published as null
- Units hardcoded (stable, avoids extra API call per series)

## Entities Published

**flux-energy** (3 entities, hourly):
| Entity | Series ID | Source |
|--------|-----------|--------|
| wti-crude | RWTC | EIA petroleum/pri/spt |
| henry-hub-natural-gas | RNGWHHD | EIA petroleum/pri/spt |
| us-electricity-demand | US48-D | EIA electricity/rto/region-data |

Properties: `value`, `unit`, `series_date`, `series_id`

**flux-economic** (5 entities, hourly):
| Entity | Series ID | Units |
|--------|-----------|-------|
| ism-manufacturing-pmi | NAPM | Index |
| ism-services-pmi | NMFCI | Index |
| us-unemployment-rate | UNRATE | Percent |
| us-inflation-cpi | CPIAUCSL | Index 1982-84=100 |
| us-gdp-growth | A191RL1Q225SBEA | Percent Change |

Properties: `value`, `series_date`, `series_id`, `units`
