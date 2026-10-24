"""Pull aviation-business candidates (likely FBOs) from Overture Maps places for the US.

Output: data/raw/overture/aviation_places.parquet
Usage: .venv/bin/python -I pipeline/fetch_overture_fbo.py [release]
"""
import sys
from pathlib import Path

import duckdb

ROOT = Path(__file__).resolve().parent.parent
RELEASE = sys.argv[1] if len(sys.argv) > 1 else "2026-10-21.0"
OUT = ROOT / "data" / "raw" / "overture" / "aviation_places.parquet"

NAME_RX = (
    r"(?i)(\bfbo\b|aviation|jet ?cent|air ?cent|\baero|airport services|flight services|air services|aircraft services|"
    r"signature|atlantic|million air|sheltair|wilson air|cutter|ross aviation|tac air|modern aviation|jet aviation|"
    r"clay lacy|avflight|banyan|lynx|xjet|pentastar|ascend|skyservice|landmark|mercury air|jetstream|avfuel|hangar|"
    r"air ?park services|fuel|flight support|executive air|air ?ways|aviators|aircraft|\bjet\b|\bair\b)"
)

CATS = ("airport", "airport_terminal", "aircraft_service", "private_jet_charter", "airline", "aircraft_parts_and_supplies",
        "flight_school", "aircraft_dealer", "airport_lounge")

OUT.parent.mkdir(parents=True, exist_ok=True)
con = duckdb.connect()
con.execute("INSTALL httpfs; LOAD httpfs; SET s3_region='us-west-2';")
cats = ",".join(f"'{c}'" for c in CATS)
con.execute(f"""
COPY (
  SELECT id, names.primary AS name, taxonomy.primary AS cat, basic_category, confidence,
         phones, websites, socials, emails, brand.names.primary AS brand,
         addresses[1].freeform AS street, addresses[1].locality AS city, addresses[1].region AS region, addresses[1].postcode AS zip,
         (bbox.xmin + bbox.xmax) / 2 AS lon, (bbox.ymin + bbox.ymax) / 2 AS lat,
         list_transform(sources, lambda s: s.dataset) AS datasets, operating_status
  FROM read_parquet('s3://overturemaps-us-west-2/release/{RELEASE}/theme=places/type=place/*', hive_partitioning=1)
  WHERE bbox.xmin BETWEEN -180 AND -64 AND bbox.ymin BETWEEN 17 AND 72
    AND (taxonomy.primary IN ({cats}) OR regexp_matches(names.primary, '{NAME_RX}'))
    AND coalesce(operating_status, 'open') <> 'permanently_closed'
) TO '{OUT}' (FORMAT parquet)
""")
print(con.execute(f"SELECT count(*) FROM '{OUT}'").fetchone()[0], "candidates")
