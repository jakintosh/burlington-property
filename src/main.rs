use anyhow::{Context, Result};
use schemafy;
use serde::{Deserialize, Serialize};
use serde_json;
use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use thiserror::Error;

// load types from json schema files
schemafy::schemafy!(root: Locations "schemas/parcel-gps-schema.json");
schemafy::schemafy!(root: Taxes "schemas/property-taxes-schema.json");
schemafy::schemafy!(root: BuildingData "schemas/building-infomation-schema.json");

struct Property {
    lot_size: Option<i64>,
    latitude: Option<f64>,
    longitude: Option<f64>,
    address: Option<String>,
}

struct TaxRecord {
    year: String,
    tax_per_sqft: f64,
    property_id: String,
}

fn main() -> Result<()> {
    println!("Loading files...");

    // load data from json files
    let buildings: Vec<PropertyBuildingDataRecords> =
        read_data_from_file("data/property-building-data.json")
            .context("failed loading buildings")?;
    let taxes: Vec<PropertyTaxesByParcelIdRecords> =
        read_data_from_file("data/property-taxes-by-parcel-id.json")
            .context("failed loading taxes")?;
    let parcels: Vec<TaxParcelGpsLocationsRecords> =
        read_data_from_file("data/tax-parcel-gps-locations.json")
            .context("failed loading parcels")?;

    let mut properties: HashMap<String, Property> = HashMap::new();

    // process building data
    println!("Processing buildings...");
    for record in buildings {
        if let Some(building) = &record.fields {
            if let Some(tax_id) = &building.taxparcelid {
                properties.insert(
                    tax_id.clone(),
                    Property {
                        lot_size: building.lotsqfeet,
                        address: building.streetaddressformatted.clone(),
                        latitude: None,
                        longitude: None,
                    },
                );
            }
        }
    }

    // process location data
    println!("Processing parcels...");
    for record in parcels {
        if let Some(parcel) = record.fields {
            if let Some(tax_id) = &parcel.taxparcelid {
                if let Some(property) = properties.get_mut(tax_id) {
                    property.latitude = parcel.latitude;
                    property.longitude = parcel.longitude;
                }
            }
        }
    }

    // process tax data
    let mut tax_records: Vec<TaxRecord> = Vec::new();
    println!("Processing taxes...");
    for record in taxes {
        if let Some(tax) = &record.fields {
            if let (Some(tax_id), Some(year), Some(amount)) =
                (&tax.taxparcelid, &tax.fiscalyear, tax.taxamount)
            {
                if let Some(property) = properties.get(tax_id) {
                    if let Some(lot_size) = property.lot_size {
                        tax_records.push(TaxRecord {
                            tax_per_sqft: tax_per_sqft(amount, lot_size),
                            year: year.clone(),
                            property_id: tax_id.clone(),
                        });
                    }
                }
            }
        }
    }

    println!("Sorting data...");
    tax_records.sort_unstable_by(|a, b| {
        b.tax_per_sqft
            .partial_cmp(&a.tax_per_sqft)
            .unwrap_or(std::cmp::Ordering::Less)
    });

    let taxes_2021: Vec<_> = tax_records
        .iter()
        .filter(|record| record.year == "2021")
        .collect();
    let mut tax_records = taxes_2021.iter().rev().take(200);
    while let Some(tax_record) = tax_records.next() {
        if let Some(property) = properties.get(&tax_record.property_id) {
            println!(
                "|  {tax:.2}  |  {year}  |  {addr:?}",
                tax = tax_record.tax_per_sqft,
                year = tax_record.year,
                addr = property.address
            );
        }
    }

    Ok(())
}

fn tax_per_sqft(amount: f64, lot_size: i64) -> f64 {
    if lot_size > 10 {
        if amount > 0.0 {
            return amount / f64::from(lot_size as i32);
        } else {
            return 0.0;
        }
    } else {
        return -1.0;
    }
}

#[derive(Debug, Error)]
enum Error {
    #[error("Couldn't open file")]
    FileOpenError { source: std::io::Error },

    #[error("Couldn't parse file")]
    ParseError { source: serde_json::Error },
}

fn read_data_from_file<T: serde::de::DeserializeOwned, P: AsRef<Path>>(
    path: P,
) -> Result<T, Error> {
    let file = File::open(path).map_err(|e| Error::FileOpenError { source: e })?;
    let reader = BufReader::new(file);
    let u = serde_json::from_reader(reader).map_err(|e| Error::ParseError { source: e })?;

    Ok(u)
}
