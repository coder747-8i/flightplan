# Getting a route into FltPlan Go without filing

Research notes (October 2026) on sending a Flightplan route to FltPlan Go automatically, and
what the app does today.

## What FltPlan offers

- **No public API.** FltPlan.com documents no API, no URL parameters for pre-filling a flight
  plan, and FltPlan Go lists no URL scheme or "Open in" handler for flight plan files. The only
  query strings on fltplan.com are its own page links (`CRN10`, `CARRYUNAME`, `MODE`), which
  carry session state, not route data.
- **FltPlan.com and FltPlan Go sync.** Flight plans and navlogs on FltPlan.com sync to the
  app automatically, and plans made in the app sync back.
- **Saving isn't filing.** On FltPlan.com, *Save* stores the plan and returns to the main menu;
  *File* saves it *and* queues it to be filed two hours before departure. A saved plan still
  syncs to FltPlan Go.
- **File import in FltPlan Go is custom waypoints only**, in **KML or CSV**. FltPlan's own
  material doesn't list Garmin `.fpl` or GPX import. The CSV column layout isn't documented,
  so Flightplan uses KML, which is a standard.

## What Flightplan does (Route → Export)

1. **QR code of the route string.** Point the iPad or iPhone camera at it, tap *Copy*, then in
   FltPlan Go start a new flight plan and paste the route. Save it; there's no need to file.
   This is the quickest path that involves no account automation and nothing sent to the FAA.
2. **FltPlan identifiers.** Lower-48 airports with a three-character FAA id get the K prefix
   FltPlan expects (C80 → KC80, SDL → KSDL; Alaska and Hawaii keep their FAA id), and lat/long
   points are written as coordinates (3430N11230W). The KML export still carries the points
   for Google Earth or FltPlan Go custom waypoints if wanted.
3. **FltPlan.com form fields**, each ready to copy: departure, destination, route, altitude
   (hundreds of feet, or FL), aircraft ID and type, TAS, ETE and fuel on board. Press *Save*,
   not *File*, and the plan appears in FltPlan Go.

## Options considered and not built

- **Driving FltPlan.com with the user's login** (a headless browser filling the form and
  pressing Save). It would work until FltPlan changes a page. It would also mean storing the
  pilot's FltPlan password and automating a site whose terms don't invite it. Not done.
- **Garmin `.fpl` to FltPlan Go.** FltPlan Go doesn't advertise `.fpl` import, so this path
  isn't promised. Flightplan still exports `.fpl` for avionics, Garmin Pilot and ForeFlight.
- **Ask FltPlan.** The cleanest automatic route would be an official import endpoint or a
  `fltplango://` URL with the route in it. If FltPlan support confirms one, wire it into
  `native/src/ui/export.rs` next to the QR code.

## Sources

- FltPlan Go on the App Store (sync, KML / CSV custom waypoints, no file-import or URL-scheme
  mention): https://apps.apple.com/us/app/-/id895187253
- FltPlan "Navlog Refresh" newsletter (Save vs File):
  https://imageserver.fltplan.com/FltBrief/October2021/FltBrief_September2021.html
- FltPlan.com link page (no integration parameters): https://www.fltplan.com/Link.htm
