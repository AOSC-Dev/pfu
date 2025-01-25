//! libabbs is a utilities library for AOSC OS packaging scripts maintenance
//! tasks.

#[cfg(feature = "apml")]
pub mod apml;
#[cfg(feature = "tree")]
pub mod tree;

/// An ISA supported by AOSC OS.
#[derive(PartialEq, Eq, Clone, Copy, Hash)]
pub enum Architecture {
	NoArch,
	Amd64,
	Arm64,
	LoongArch64,
	Riscv,
	Loongson3,
	Ppc64el,
}

impl Architecture {
	/// Returns the identifier for the architecture.
	pub fn ident(&self) -> &'static str {
		match self {
			Architecture::NoArch => "noarch",
			Architecture::Amd64 => "amd64",
			Architecture::Arm64 => "arm64",
			Architecture::LoongArch64 => "loongarch64",
			Architecture::Riscv => "riscv",
			Architecture::Loongson3 => "loongson3",
			Architecture::Ppc64el => "ppc64el",
		}
	}

	/// Recognizes an architecture identifier.
	pub fn from_ident(ident: &str) -> Option<Self> {
		match ident {
			"noarch" => Some(Self::NoArch),
			"amd64" => Some(Self::Amd64),
			"arm64" => Some(Self::Arm64),
			"loongarch64" => Some(Self::LoongArch64),
			"riscv" => Some(Self::Riscv),
			"loongson3" => Some(Self::Loongson3),
			"ppc64el" => Some(Self::Ppc64el),
			_ => None,
		}
	}

	/// Returns if the architecture is [`noarch`][Self::NoArch].
	pub fn is_noarch(&self) -> bool {
		matches!(self, Self::NoArch)
	}
}
