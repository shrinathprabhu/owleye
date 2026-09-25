# Country boundaries

`world-countries.json` uses the original geographic coordinates from
https://cdn.amcharts.com/lib/5/geodata/json/worldIndiaLow.json (retrieved 2026-09-19),
the same source family as the existing SVG maps. Antarctica is excluded. Country
names are kept in `properties.label`; ISO codes identify regions in `properties.name`.
See LICENSE.amcharts. The geography panel displays the amCharts attribution link.

The geographic coordinates are shared by the flat map and orthographic globe;
no inverse approximation of the old Mercator SVG is needed.
