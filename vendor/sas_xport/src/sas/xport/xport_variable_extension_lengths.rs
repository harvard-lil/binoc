pub(crate) struct XportVariableExtensionLengths {
    name: u16,
    label: u16,
    format: u16,
    input_format: u16,
}

impl XportVariableExtensionLengths {
    pub fn new(name: u16, label: u16, format: u16, input_format: u16) -> Self {
        Self {
            name,
            label,
            format,
            input_format,
        }
    }

    #[must_use]
    pub fn name(&self) -> usize {
        self.name as usize
    }

    #[must_use]
    pub fn label(&self) -> usize {
        self.label as usize
    }

    #[must_use]
    pub fn format(&self) -> usize {
        self.format as usize
    }

    #[must_use]
    pub fn input_format(&self) -> usize {
        self.input_format as usize
    }

    #[must_use]
    pub fn total_length(&self) -> usize {
        let mut total = self.name as usize;
        total += self.label as usize;
        total += self.format as usize;
        total += self.input_format as usize;
        total
    }
}
