# Aircraft files (`.fpaircraft.json`)

Flightplan exports and imports airframes as JSON. One file can hold one aircraft or a whole
fleet. Use **Aircraft → Export** to make one, **Aircraft → Import** to read one, or
**Import → Get a template file** for a blank to fill in by hand.

Importing an aircraft whose tail number is already in your hangar updates that aircraft
(keeping the current loading) instead of adding a copy.

```json
{
  "format": "flightplan-aircraft",
  "version": 1,
  "exported": "2026-10-24T18:00:00.000Z",
  "aircraft": [
    {
      "name": "Cessna 172S Skyhawk",
      "tail": "N12345",
      "type": "C172",
      "fuelType": "100LL",
      "lbPerGal": 6,
      "usableFuel": 53,
      "perf": {
        "tas": 120, "gph": 8.5,
        "climbFpm": 700, "climbTas": 75, "climbGph": 10.5,
        "descentFpm": 500, "descentTas": 120, "descentGph": 7,
        "taxiGal": 1.1, "reserveMin": 30, "maxXwind": 15
      },
      "wb": {
        "emptyWeight": 1663, "emptyArm": 41.0,
        "maxRamp": 2558, "maxTakeoff": 2550, "maxLanding": 2550,
        "stations": [
          { "id": "front", "name": "Pilot & front passenger", "arm": 37.0, "kind": "seat" },
          { "id": "rear", "name": "Rear passengers", "arm": 73.0, "kind": "seat" },
          { "id": "bagA", "name": "Baggage area A", "arm": 95.0, "kind": "baggage", "max": 120 },
          { "id": "fuel", "name": "Fuel (usable)", "arm": 48.0, "kind": "fuel", "max": 53 }
        ],
        "envelope": [[35.0, 1500], [35.0, 1950], [41.0, 2550], [47.3, 2550], [47.3, 1500]],
        "utility": [[35.0, 1500], [35.0, 1950], [37.5, 2200], [40.5, 2200], [40.5, 1500]]
      },
      "notes": "Anything you want on hand"
    }
  ]
}
```

Units: pounds, inches aft of the datum, US gallons, knots, feet per minute.

| Field | Required | Notes |
| --- | --- | --- |
| `name`, `tail`, `type` | no | `type` is the ICAO designator (C172, P28A, SR22, ...) |
| `fuelType` | no | `100LL`, `UL94`, `Mogas` or `Jet A` (default 100LL) |
| `lbPerGal` | no | default 6, or 6.7 for Jet A |
| `usableFuel` | yes* | gallons; *taken from the fuel station's `max` when missing |
| `perf.*` | no | missing values get defaults, and the import says which |
| `wb.emptyWeight`, `wb.emptyArm` | yes | from the airplane's current weight & balance record |
| `wb.maxTakeoff` | yes | `maxRamp` and `maxLanding` default to it |
| `wb.stations` | yes | each needs `name` and `arm`; `kind` is `seat`, `baggage`, `fuel` or `other` (guessed from the name when missing); `max` is lb, or gallons for fuel. At least one fuel station. |
| `wb.envelope` | yes | at least 3 `[arm, weight]` corner points in order around the outline; `{ "arm": .., "weight": .. }` objects work too |
| `wb.utility` | no | utility-category envelope, same shape |

The import also accepts a bare aircraft object or a JSON array of them. Each entry is checked:
errors (missing weights, no fuel station, too few envelope points) stop that aircraft from being
added; warnings (defaulted performance numbers, an envelope that doesn't reach max takeoff
weight, station arms far outside the envelope) are shown but don't.

In the step-by-step editor, the Stations and Envelope steps also take rows pasted from a
spreadsheet or POH table (tabs, commas or semicolons).

Templates and examples use typical POH numbers. Your airplane's weighing record and POH govern.
