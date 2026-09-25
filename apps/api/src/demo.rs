//! Legacy analytics fixtures. No authentication path grants this identity.
//! The normal sample account is seeded into SQLite and ClickHouse instead.

pub(crate) const DEMO_USER_ID: &str = "a9be8cb0-3af3-4f4e-a33b-b748bc2c9137";
pub(crate) const PREVIEW_EMAIL: &str = "fixture@example.invalid";
pub(crate) const PREVIEW_MEMBER_MAYA_ID: &str = "80ac4a63-a7ec-45f9-b768-d83473cf36be";
pub(crate) const PREVIEW_MEMBER_SAM_ID: &str = "c50e8375-f14c-4e23-90ae-67a75832e9d7";
pub(crate) const PREVIEW_NAME: &str = "Analytics fixture";
pub(crate) const PREVIEW_ORGANIZATION_ID: &str = "33c90142-415f-44f0-bfe5-a12201d5bb23";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DemoTrafficProfile {
    Campaign,
    LowVolume,
    Massive,
    Mixed,
    New,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct DemoSiteDefinition {
    pub created_days_ago: i64,
    pub domain: &'static str,
    pub id: &'static str,
    pub name: &'static str,
    pub profile: DemoTrafficProfile,
    pub public_key: &'static str,
    pub timezone: &'static str,
    pub tracking_id: &'static str,
}

pub(crate) const DEMO_SITES: [DemoSiteDefinition; 5] = [
    DemoSiteDefinition {
        created_days_ago: 540,
        domain: "nova.example",
        id: "c4c851a7-53ad-4b23-a4f9-976dddb64f52",
        name: "Nova Commerce",
        profile: DemoTrafficProfile::Massive,
        public_key: "owl_pub_50e85b4de15a473aa1ba1f0e3890dd4f",
        timezone: "America/New_York",
        tracking_id: "owl_8f25c184d6aa4be6a59f43e09a3f671b",
    },
    DemoSiteDefinition {
        created_days_ago: 310,
        domain: "pocket.example",
        id: "74e31a93-e0c8-4232-b3d4-80c48d15ba5e",
        name: "Pocket Studio",
        profile: DemoTrafficProfile::LowVolume,
        public_key: "owl_pub_9855279646f04efb864f116eb803d488",
        timezone: "Europe/London",
        tracking_id: "owl_74c86efab2894205ae193f9d0e1db784",
    },
    DemoSiteDefinition {
        created_days_ago: 220,
        domain: "launchpad.example",
        id: "e40dc59e-62c7-4afa-8951-12f455703ca0",
        name: "Launchpad Campaigns",
        profile: DemoTrafficProfile::Campaign,
        public_key: "owl_pub_0fd64d4f353b4643aee4ae5451b1420f",
        timezone: "Asia/Kolkata",
        tracking_id: "owl_232a551ee68f42f4aa69bc290a3624a9",
    },
    DemoSiteDefinition {
        created_days_ago: 6,
        domain: "sprout.example",
        id: "5e74e41c-df8e-4341-9783-10f2ad22da3b",
        name: "Sprout Notes",
        profile: DemoTrafficProfile::New,
        public_key: "owl_pub_873bc55f1b16407d8a6ed8e9bc07e1cd",
        timezone: "Australia/Sydney",
        tracking_id: "owl_ee22f865085e4ecb98b2abbfa120c9ed",
    },
    DemoSiteDefinition {
        created_days_ago: 410,
        domain: "atlas.example",
        id: "197508c4-76b5-4426-a0eb-039037039f22",
        name: "Atlas Docs",
        profile: DemoTrafficProfile::Mixed,
        public_key: "owl_pub_b9f652970301411fa05fc20c2f42c1fd",
        timezone: "UTC",
        tracking_id: "owl_6995fcafb1cc47eb986b661f1623b352",
    },
];

pub(crate) fn demo_site(identifier: &str) -> Option<&'static DemoSiteDefinition> {
    DEMO_SITES
        .iter()
        .find(|site| site.id == identifier || site.tracking_id == identifier)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_catalog_has_five_unique_sites_and_all_profiles() {
        assert_eq!(DEMO_SITES.len(), 5);
        for (index, site) in DEMO_SITES.iter().enumerate() {
            assert!(DEMO_SITES[index + 1..]
                .iter()
                .all(|other| other.id != site.id && other.tracking_id != site.tracking_id));
        }
        assert!(DEMO_SITES
            .iter()
            .any(|site| site.profile == DemoTrafficProfile::Massive));
        assert!(DEMO_SITES
            .iter()
            .any(|site| site.profile == DemoTrafficProfile::LowVolume));
        assert!(DEMO_SITES
            .iter()
            .any(|site| site.profile == DemoTrafficProfile::Campaign));
        assert!(DEMO_SITES
            .iter()
            .any(|site| site.profile == DemoTrafficProfile::New));
    }
}
