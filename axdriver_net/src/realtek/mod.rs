//! RealTek Ethernet Driver
//!
//! This module provides driver implementations for RealTek network adapters.
//! Supports RTL8139 (Fast Ethernet) and RTL8169/RTL8168/RTL8111 (Gigabit Ethernet) series.

mod device_info;
mod kernel_func;
mod regs;
mod rtl8139;
mod rtl8169;

use crate::{EthernetAddress, NetBufPtr, NetDriverOps};
use axdriver_base::{BaseDriverOps, DevError, DevResult, DeviceType};

pub use device_info::{RealtekDeviceInfo, RealtekSeries, REALTEK_DEVICES};
pub use kernel_func::KernelFunc;
pub use kernel_func::UseKernelFunc;
pub use rtl8139::Rtl8139Driver;
pub use rtl8169::Rtl8169Driver;

/// RealTek unified driver enum
pub enum RealtekDriver {
    Rtl8139(Rtl8139Driver),
    Rtl8169(Rtl8169Driver),
}

impl BaseDriverOps for RealtekDriver {
    fn device_name(&self) -> &str {
        match self {
            RealtekDriver::Rtl8139(driver) => driver.device_name(),
            RealtekDriver::Rtl8169(driver) => driver.device_name(),
        }
    }

    fn device_type(&self) -> DeviceType {
        DeviceType::Net
    }
}

impl NetDriverOps for RealtekDriver {
    fn mac_address(&self) -> EthernetAddress {
        match self {
            RealtekDriver::Rtl8139(driver) => driver.mac_address(),
            RealtekDriver::Rtl8169(driver) => driver.mac_address(),
        }
    }

    fn can_transmit(&self) -> bool {
        match self {
            RealtekDriver::Rtl8139(driver) => driver.can_transmit(),
            RealtekDriver::Rtl8169(driver) => driver.can_transmit(),
        }
    }

    fn can_receive(&self) -> bool {
        match self {
            RealtekDriver::Rtl8139(driver) => driver.can_receive(),
            RealtekDriver::Rtl8169(driver) => driver.can_receive(),
        }
    }

    fn rx_queue_size(&self) -> usize {
        match self {
            RealtekDriver::Rtl8139(driver) => driver.rx_queue_size(),
            RealtekDriver::Rtl8169(driver) => driver.rx_queue_size(),
        }
    }

    fn tx_queue_size(&self) -> usize {
        match self {
            RealtekDriver::Rtl8139(driver) => driver.tx_queue_size(),
            RealtekDriver::Rtl8169(driver) => driver.tx_queue_size(),
        }
    }

    fn recycle_rx_buffer(&mut self, rx_buf: NetBufPtr) -> DevResult {
        match self {
            RealtekDriver::Rtl8139(driver) => driver.recycle_rx_buffer(rx_buf),
            RealtekDriver::Rtl8169(driver) => driver.recycle_rx_buffer(rx_buf),
        }
    }

    fn recycle_tx_buffers(&mut self) -> DevResult {
        match self {
            RealtekDriver::Rtl8139(driver) => driver.recycle_tx_buffers(),
            RealtekDriver::Rtl8169(driver) => driver.recycle_tx_buffers(),
        }
    }

    fn transmit(&mut self, tx_buf: NetBufPtr) -> DevResult {
        match self {
            RealtekDriver::Rtl8139(driver) => driver.transmit(tx_buf),
            RealtekDriver::Rtl8169(driver) => driver.transmit(tx_buf),
        }
    }

    fn receive(&mut self) -> DevResult<NetBufPtr> {
        match self {
            RealtekDriver::Rtl8139(driver) => driver.receive(),
            RealtekDriver::Rtl8169(driver) => driver.receive(),
        }
    }

    fn alloc_tx_buffer(&mut self, size: usize) -> DevResult<NetBufPtr> {
        match self {
            RealtekDriver::Rtl8139(driver) => driver.alloc_tx_buffer(size),
            RealtekDriver::Rtl8169(driver) => driver.alloc_tx_buffer(size),
        }
    }
}

/// Check if PCI device is a RealTek controller
pub fn is_realtek_device(vendor_id: u16, device_id: u16) -> bool {
    REALTEK_DEVICES
        .iter()
        .any(|info| info.vendor_id == vendor_id && info.device_id == device_id)
}

/// Get RealTek device information
pub fn get_device_info(vendor_id: u16, device_id: u16) -> Option<&'static RealtekDeviceInfo> {
    REALTEK_DEVICES
        .iter()
        .find(|info| info.vendor_id == vendor_id && info.device_id == device_id)
}

/// Create RealTek driver from PCI device information
pub fn create_driver(
    vendor_id: u16,
    device_id: u16,
    base_addr: usize,
    irq: u8,
) -> DevResult<RealtekDriver> {
    let device_info = get_device_info(vendor_id, device_id).ok_or(DevError::InvalidParam)?;

    log::info!(
        "Creating RealTek driver: {} (vendor: {:#x}, device: {:#x})",
        device_info.name,
        vendor_id,
        device_id
    );

    let driver = match device_info.series {
        RealtekSeries::Rtl8139 => {
            let mut drv = Rtl8139Driver::new(base_addr, irq)?;
            drv.init()?;
            RealtekDriver::Rtl8139(drv)
        }
        RealtekSeries::Rtl8169 | RealtekSeries::Rtl8168 | RealtekSeries::Rtl8111 => {
            let mut drv = Rtl8169Driver::new(base_addr, irq, device_info.series)?;
            drv.init()?;
            RealtekDriver::Rtl8169(drv)
        }
    };

    log::info!("RealTek driver initialized successfully");

    Ok(driver)
}
