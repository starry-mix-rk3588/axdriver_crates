//! RTL8169 PHY and MDIO management

use log::*;

use super::regs::{Reg, Rtl8169Regs};

/// PHY register addresses
#[allow(dead_code)]
pub mod phy_reg {
    pub const BMCR: u8 = 0; // Basic Mode Control Register
    pub const BMSR: u8 = 1; // Basic Mode Status Register
    pub const ANAR: u8 = 4; // Auto-negotiation Advertisement
    pub const GBCR: u8 = 9; // 1000BASE-T Control Register
}

/// BMCR (Basic Mode Control Register) bits
#[allow(dead_code)]
pub mod bmcr {
    pub const RESET: u16 = 0x8000;
    pub const LOOPBACK: u16 = 0x4000;
    pub const SPEED_SEL_LSB: u16 = 0x2000; // Speed select (LSB)
    pub const AN_ENABLE: u16 = 0x1000; // Auto-negotiation enable
    pub const POWER_DOWN: u16 = 0x0800;
    pub const ISOLATE: u16 = 0x0400;
    pub const AN_RESTART: u16 = 0x0200; // Restart auto-negotiation
    pub const DUPLEX_MODE: u16 = 0x0100; // Full duplex
    pub const COLLISION_TEST: u16 = 0x0080;
    pub const SPEED_SEL_MSB: u16 = 0x0040; // Speed select (MSB)
}

/// BMSR (Basic Mode Status Register) bits
#[allow(dead_code)]
pub mod bmsr {
    pub const _100BASE_T4: u16 = 0x8000;
    pub const _100BASE_TX_FULL: u16 = 0x4000;
    pub const _100BASE_TX_HALF: u16 = 0x2000;
    pub const _10BASE_T_FULL: u16 = 0x1000;
    pub const _10BASE_T_HALF: u16 = 0x0800;
    pub const AN_COMPLETE: u16 = 0x0020; // Auto-negotiation complete
    pub const REMOTE_FAULT: u16 = 0x0010;
    pub const AN_CAPABLE: u16 = 0x0008; // Auto-negotiation capable
    pub const LINK_STATUS: u16 = 0x0004; // Link status
    pub const JABBER_DETECT: u16 = 0x0002;
    pub const EXTENDED_CAP: u16 = 0x0001;
}

/// ANAR (Auto-negotiation Advertisement) bits
#[allow(dead_code)]
pub mod anar {
    pub const NEXT_PAGE: u16 = 0x8000;
    pub const REMOTE_FAULT: u16 = 0x2000;
    pub const PAUSE: u16 = 0x0400;
    pub const _100BASE_T4: u16 = 0x0200;
    pub const _100BASE_TX_FULL: u16 = 0x0100;
    pub const _100BASE_TX_HALF: u16 = 0x0080;
    pub const _10BASE_T_FULL: u16 = 0x0040;
    pub const _10BASE_T_HALF: u16 = 0x0020;
    pub const SELECTOR: u16 = 0x001F;
}

/// GBCR (1000BASE-T Control) bits
#[allow(dead_code)]
pub mod gbcr {
    pub const _1000BASE_T_FULL: u16 = 0x0200;
    pub const _1000BASE_T_HALF: u16 = 0x0100;
}

/// PHY manager
pub struct PhyManager<'a> {
    regs: &'a Rtl8169Regs,
}

impl<'a> PhyManager<'a> {
    /// Create new PHY manager
    pub fn new(regs: &'a Rtl8169Regs) -> Self {
        Self { regs }
    }

    /// Read from PHY register via MDIO
    pub fn mdio_read(&self, reg_addr: u8) -> u16 {
        // Write PHYAR register to initiate read
        self.regs.write32(Reg::PhyAr, (reg_addr as u32) << 16);

        // Wait for read to complete (busy wait with timeout)
        for _ in 0..2000 {
            let val = self.regs.read32(Reg::PhyAr);
            if (val & 0x80000000) != 0 {
                return (val & 0xFFFF) as u16;
            }
            // Small delay
            for _ in 0..100 {
                core::hint::spin_loop();
            }
        }

        warn!("RTL8169: MDIO read timeout for reg {}", reg_addr);
        0xFFFF
    }

    /// Write to PHY register via MDIO
    pub fn mdio_write(&self, reg_addr: u8, value: u16) {
        // Write PHYAR register with write bit and data
        self.regs.write32(
            Reg::PhyAr,
            0x80000000 | ((reg_addr as u32) << 16) | (value as u32),
        );

        // Wait for write to complete
        for _ in 0..2000 {
            let val = self.regs.read32(Reg::PhyAr);
            if (val & 0x80000000) == 0 {
                return;
            }
            // Small delay
            for _ in 0..100 {
                core::hint::spin_loop();
            }
        }

        warn!("RTL8169: MDIO write timeout for reg {}", reg_addr);
    }

    /// Initialize PHY and perform auto-negotiation
    pub fn init_and_negotiate(&self) -> Result<(), &'static str> {
        info!("RTL8169: Initializing PHY");

        // Read current auto-negotiation advertisement
        let anar = self.mdio_read(phy_reg::ANAR);
        debug!("RTL8169: Current ANAR: {:#x}", anar);

        // Configure auto-negotiation: advertise all speeds
        let new_anar = anar::_10BASE_T_HALF
            | anar::_10BASE_T_FULL
            | anar::_100BASE_TX_HALF
            | anar::_100BASE_TX_FULL
            | (anar & 0x1F); // Keep selector bits

        self.mdio_write(phy_reg::ANAR, new_anar);
        debug!("RTL8169: Set ANAR to {:#x}", new_anar);

        // Enable 1000BASE-T full duplex
        self.mdio_write(phy_reg::GBCR, gbcr::_1000BASE_T_FULL);
        debug!("RTL8169: Set GBCR to {:#x}", gbcr::_1000BASE_T_FULL);

        // Enable auto-negotiation and restart
        self.mdio_write(phy_reg::BMCR, bmcr::AN_ENABLE | bmcr::AN_RESTART);
        info!("RTL8169: Started auto-negotiation");

        // Wait for auto-negotiation to complete
        let mut timeout = 10000;
        while timeout > 0 {
            let bmsr = self.mdio_read(phy_reg::BMSR);
            if (bmsr & bmsr::AN_COMPLETE) != 0 {
                info!("RTL8169: Auto-negotiation complete");

                // Read negotiated speed/duplex from PHY status
                let phy_status = self.regs.read8(Reg::PhyStatus);
                self.log_link_status(phy_status);

                return Ok(());
            }

            timeout -= 1;
            // Small delay (~100us)
            for _ in 0..100 {
                core::hint::spin_loop();
            }
        }

        warn!("RTL8169: Auto-negotiation timeout, checking link anyway");
        let phy_status = self.regs.read8(Reg::PhyStatus);
        self.log_link_status(phy_status);

        Ok(())
    }

    /// Log current link status
    fn log_link_status(&self, phy_status: u8) {
        use super::regs::phy_status::*;

        if (phy_status & LINK_STATUS) != 0 {
            let speed = if (phy_status & _1000BPSF) != 0 {
                "1000Mbps"
            } else if (phy_status & _100BPS) != 0 {
                "100Mbps"
            } else if (phy_status & _10BPS) != 0 {
                "10Mbps"
            } else {
                "Unknown"
            };

            let duplex = if (phy_status & FULL_DUP) != 0 {
                "Full-duplex"
            } else {
                "Half-duplex"
            };

            info!("RTL8169: Link UP - {} {}", speed, duplex);
        } else {
            warn!("RTL8169: Link DOWN");
        }
    }

    /// Check if link is up
    pub fn is_link_up(&self) -> bool {
        use super::regs::phy_status::LINK_STATUS;
        let phy_status = self.regs.read8(Reg::PhyStatus);
        (phy_status & LINK_STATUS) != 0
    }
}
