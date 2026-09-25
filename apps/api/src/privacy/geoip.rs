use std::{net::IpAddr, path::Path};

use maxminddb::{geoip2, Reader};

pub struct GeoIp {
    reader: Option<Reader<Vec<u8>>>,
}

#[derive(Clone, Default)]
pub struct GeoLocation {
    pub city: Option<String>,
    pub continent: Option<String>,
    pub country: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub region: Option<String>,
}

impl GeoIp {
    pub fn open(path: Option<&Path>) -> anyhow::Result<Self> {
        let reader = match path {
            Some(path) => Some(Reader::open_readfile(path)?),
            None => None,
        };

        Ok(Self { reader })
    }

    pub fn lookup(&self, ip: IpAddr) -> GeoLocation {
        let Some(reader) = &self.reader else {
            return GeoLocation::default();
        };

        let Ok(lookup) = reader.lookup(ip) else {
            return GeoLocation::default();
        };
        let Ok(Some(city)) = lookup.decode::<geoip2::City<'_>>() else {
            return GeoLocation::default();
        };

        let city_name = city.city.names.english.map(str::to_owned);
        let continent = city.continent.code.map(str::to_owned);
        let country = city.country.iso_code.map(str::to_owned);
        let region = city
            .subdivisions
            .first()
            .and_then(|subdivision| subdivision.iso_code.map(str::to_owned));
        let latitude = city.location.latitude;
        let longitude = city.location.longitude;

        GeoLocation {
            city: city_name,
            continent,
            country,
            latitude,
            longitude,
            region,
        }
    }
}
