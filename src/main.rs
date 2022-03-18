use anyhow::{Context, Result};
use schemafy;
use serde::{Deserialize, Serialize};
use serde_json;
use std::collections::HashMap;
use std::fmt::Display;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use thiserror::Error;

// load types from json schema files
schemafy::schemafy!(root: Buildings "schemas/building-infomation-schema.json");
schemafy::schemafy!(root: Taxes "schemas/property-taxes-schema.json");
schemafy::schemafy!(root: Parcels "schemas/parcel-gps-schema.json");

#[derive(Debug, Error)]
enum Error {
    #[error("Couldn't open file")]
    FileOpenError { source: std::io::Error },

    #[error("Couldn't parse file")]
    ParseError { source: serde_json::Error },
}

struct Property {
    lot_size: Option<i64>,
    address: Option<String>,
    latitude: Option<f64>,
    longitude: Option<f64>,
}

struct TaxRecord {
    tax_parcel_id: String,
    year: String,
    tax_per_sqft: TaxRatio,
    taxes_paid: f64,
    lot_size: i64,
}

#[derive(Debug)]
enum TaxRatio {
    Amount(f64),
    InvalidLotSize,
    ZeroTaxPaid,
}
impl Display for TaxRatio {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TaxRatio::Amount(amt) => write!(f, "$ {:.2} / sqft", amt),
            TaxRatio::InvalidLotSize => write!(f, "Invalid Lot"),
            TaxRatio::ZeroTaxPaid => write!(f, "$0"),
        }
    }
}

fn main() -> Result<()> {
    println!("Loading buildings...");
    let buildings: Vec<PropertyBuildingDataRecords> =
        read_data_from_file("data/property-building-data.json")
            .context("failed loading buildings")?;

    println!("Loading taxes...");
    let taxes: Vec<PropertyTaxesByParcelIdRecords> =
        read_data_from_file("data/property-taxes-by-parcel-id.json")
            .context("failed loading taxes")?;

    println!("Loading parcels...");
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
                            tax_parcel_id: tax_id.clone(),
                            taxes_paid: amount,
                            lot_size: lot_size,
                        });
                    }
                }
            }
        }
    }

    println!("Sorting data...");

    // remove invalid pieces of data
    let mut invalid_lots: u32 = 0;
    let mut zero_taxes: u32 = 0;
    tax_records.retain(|r| match r.tax_per_sqft {
        TaxRatio::Amount(_) => true,
        TaxRatio::InvalidLotSize => {
            invalid_lots += 1;
            false
        }
        TaxRatio::ZeroTaxPaid => {
            zero_taxes += 1;
            false
        }
    });

    // sort valid data by rating
    tax_records.sort_unstable_by(|a, b| {
        if let (TaxRatio::Amount(a), TaxRatio::Amount(b)) = (&a.tax_per_sqft, &b.tax_per_sqft) {
            return b.partial_cmp(&a).unwrap_or(std::cmp::Ordering::Less);
        } else {
            return std::cmp::Ordering::Less;
        }
    });

    let taxes_2021: Vec<_> = tax_records
        .iter()
        .filter(|record| record.year == "2021")
        .collect();
    let mut tax_records = taxes_2021.iter().take(200);

    // print out results
    println!("Number of Invalid Lot Sizes: {}", invalid_lots);
    println!("Number of Zero Tax Payments: {}", zero_taxes);
    while let Some(tax_record) = tax_records.next() {
        if let Some(property) = properties.get(&tax_record.tax_parcel_id) {
            println!(
                "|  {tax}  |  $ {amt}  |  {lot} sqft  |  {year}  |  {addr:?}",
                tax = tax_record.tax_per_sqft,
                amt = tax_record.taxes_paid,
                lot = tax_record.lot_size,
                year = tax_record.year,
                addr = property.address
            );
        }
    }

    Ok(())
}

fn tax_per_sqft(amount: f64, lot_size: i64) -> TaxRatio {
    if lot_size < 10 {
        return TaxRatio::InvalidLotSize;
    }
    if amount <= 0.0 {
        return TaxRatio::ZeroTaxPaid;
    }

    TaxRatio::Amount(amount / f64::from(lot_size as i32))
}

fn read_data_from_file<T: serde::de::DeserializeOwned, P: AsRef<Path>>(
    path: P,
) -> Result<T, Error> {
    let file = File::open(path).map_err(|e| Error::FileOpenError { source: e })?;
    let reader = BufReader::new(file);
    let data = serde_json::from_reader(reader).map_err(|e| Error::ParseError { source: e })?;

    Ok(data)
}
