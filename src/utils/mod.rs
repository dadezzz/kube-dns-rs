use std::{
    net::{Ipv4Addr, Ipv6Addr},
    sync::Arc,
};

use hickory_server::{
    proto::{
        op::ResponseCode,
        rr::{self, Name, RecordSet, RecordType},
    },
    zone_handler::{AuthLookup, LookupControlFlow, LookupError, LookupOptions, LookupRecords},
};

#[must_use]
pub fn handler_search_aggregator(
    lookups: Vec<LookupControlFlow<AuthLookup>>,
    lookup_options: LookupOptions,
    nxdomain: bool,
) -> LookupControlFlow<AuthLookup> {
    // Used to distinguish between returning a skip to signal the domain is
    // not managed by this handler vs returning an empty response.
    let mut zone_exists = false;

    let mut authority_records = Vec::new();
    let mut additional_records = Vec::new();

    for result in lookups {
        match result {
            LookupControlFlow::Continue(Ok(mut auth_lookup)) => {
                zone_exists = true;

                let additionals = auth_lookup.take_additionals();

                match additionals {
                    Some(LookupRecords::ManyRecords(_, records)) => {
                        additional_records.extend(records);
                    }
                    Some(LookupRecords::Records { records, .. }) => {
                        additional_records.push(records);
                    }
                    _ => {}
                }

                let authorities = auth_lookup.unwrap_records();

                match authorities {
                    LookupRecords::ManyRecords(_, records) => authority_records.extend(records),
                    LookupRecords::Records { records, .. } => authority_records.push(records),
                    _ => {}
                }
            }
            LookupControlFlow::Skip => {}
            err_or_break => return err_or_break,
        }
    }

    // Zone was not handled by this handler.
    if !zone_exists {
        return if nxdomain {
            break_with_nxdomain()
        } else {
            LookupControlFlow::Skip
        };
    }

    let authorities = LookupRecords::many(lookup_options, authority_records);
    let additionals = Some(LookupRecords::many(lookup_options, additional_records));
    let auth_lookup = AuthLookup::answers(authorities, additionals);
    LookupControlFlow::Continue(Ok(auth_lookup))
}

#[must_use]
pub fn new_a_record_set<N, I>(name: N, addresses: I, ttl: u32) -> Option<RecordSet>
where
    N: Into<Name>,
    I: IntoIterator<Item = Ipv4Addr>,
{
    let mut record_set = RecordSet::with_ttl(name.into(), RecordType::A, ttl);

    addresses
        .into_iter()
        .map(|ip| rr::RData::A(rr::rdata::A(ip)))
        .for_each(|rdata| {
            record_set.add_rdata(rdata);
        });

    if record_set.is_empty() {
        return None;
    }

    Some(record_set)
}

#[must_use]
pub fn new_aaaa_record_set<N, I>(name: N, addresses: I, ttl: u32) -> Option<RecordSet>
where
    N: Into<Name>,
    I: IntoIterator<Item = Ipv6Addr>,
{
    let mut record_set = RecordSet::with_ttl(name.into(), RecordType::AAAA, ttl);

    addresses
        .into_iter()
        .map(|ip| rr::RData::AAAA(rr::rdata::AAAA(ip)))
        .for_each(|rdata| {
            record_set.add_rdata(rdata);
        });

    if record_set.is_empty() {
        return None;
    }

    Some(record_set)
}

#[must_use]
pub fn break_with_nxdomain() -> LookupControlFlow<AuthLookup> {
    LookupControlFlow::Break(Err(LookupError::ResponseCode(ResponseCode::NXDomain)))
}

#[must_use]
pub fn continue_with_recordset(
    lookup_options: LookupOptions,
    authority_record_set: RecordSet,
    additional_record_set: Option<RecordSet>,
) -> LookupControlFlow<AuthLookup> {
    let authorities = LookupRecords::new(lookup_options, Arc::new(authority_record_set));
    let additionals =
        additional_record_set.map(|rs| LookupRecords::new(lookup_options, Arc::new(rs)));

    LookupControlFlow::Continue(Ok(AuthLookup::answers(authorities, additionals)))
}

#[must_use]
pub fn name_to_labels(name: &Name) -> Vec<&str> {
    name.iter().map(|l| str::from_utf8(l).unwrap()).collect()
}
