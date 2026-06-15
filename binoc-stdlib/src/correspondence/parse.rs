use binoc_sdk::{
    tabular_v1, BinocError, BinocResult, DataAccess, ItemRef, NodeMatch, ParseDescriptor,
    ParseOutput, ParseRule, TabularData,
};

pub struct CsvParse;

impl ParseRule for CsvParse {
    fn descriptor(&self) -> ParseDescriptor {
        ParseDescriptor {
            name: "binoc.parse.csv".into(),
            input: NodeMatch {
                is_dir: Some(false),
                extensions: vec![".csv".into(), ".tsv".into()],
                ..NodeMatch::default()
            },
            output: tabular_v1(),
            requires_link: true,
            fires_beneath_settled: false,
        }
    }

    fn parse(&self, item: &ItemRef, data: &dyn DataAccess) -> BinocResult<ParseOutput> {
        let bytes = data.read_bytes(item)?;
        let tabular = parse_csv_bytes(&bytes, delimiter_for(item))?;
        serde_json::to_vec(&tabular)
            .map(Into::into)
            .map_err(|err| BinocError::Other(format!("serialize tabular artifact: {err}")))
    }
}

fn delimiter_for(item: &ItemRef) -> u8 {
    match item.extension().as_deref() {
        Some(".tsv") => b'\t',
        _ => b',',
    }
}

fn parse_csv_bytes(bytes: &[u8], delimiter: u8) -> BinocResult<TabularData> {
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .flexible(true)
        .from_reader(bytes);
    let headers = reader
        .byte_headers()
        .map_err(|err| BinocError::Csv(err.to_string()))?
        .iter()
        .map(|field| String::from_utf8_lossy(field).into_owned())
        .collect();
    let mut rows = Vec::new();
    let mut record = csv::ByteRecord::new();
    while reader
        .read_byte_record(&mut record)
        .map_err(|err| BinocError::Csv(err.to_string()))?
    {
        rows.push(
            record
                .iter()
                .map(|field| String::from_utf8_lossy(field).into_owned())
                .collect(),
        );
    }
    Ok(TabularData { headers, rows })
}
