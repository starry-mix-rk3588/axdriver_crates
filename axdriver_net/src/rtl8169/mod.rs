//! RTL8169 PCI Gigabit Ethernet driver for StarryOS
//!
//! Based on RealTek RTL8169/8110 series Gigabit Ethernet controllers.
//! This driver supports PCI/PCIe cards and implements polling mode operation.
//!
//! # Features
//! - PCI device auto-detection
//! - MDIO PHY management and auto-negotiation
//! - 10/100/1000 Mbps speed support
//! - Polling mode TX/RX (no interrupts)
//! - Ring buffer descriptor management
//!
//! # Supported devices
//! - RTL8169/8110 series
//! - RTL8168/8111 series
//! - RTL8101E, RTL8100E

extern crate alloc;

mod desc;
mod phy;
mod regs;

use alloc::{boxed::Box, sync::Arc, vec::Vec};
use core::sync::atomic::AtomicBool;

use axdriver_base::{BaseDriverOps, DevError, DevResult, DeviceType};
use desc::{RxDesc, TxDesc};
use log::*;
use phy::PhyManager;
use regs::*;

use crate::{EthernetAddress, NetBuf, NetBufBox, NetBufPool, NetBufPtr, NetDriverOps};

/// Platform-specific kernel functions trait
///
/// This trait must be implemented by the platform integration layer
#[crate_interface::def_interface]
pub trait KernelFunc {
    /// Convert virtual address to physical address
    fn virt_to_phys(addr: usize) -> usize;

    /// Allocate DMA-coherent memory (page-aligned)
    /// Returns (virtual_address, physical_address) tuple
    fn dma_alloc_coherent(pages: usize) -> (usize, usize);

    /// Free DMA-coherent memory
    fn dma_free_coherent(vaddr: usize, pages: usize);
}

/// Configuration constants
const NUM_TX_DESC: usize = 4; // Number of TX descriptors (keep small for polling mode)
const NUM_RX_DESC: usize = 16; // Number of RX descriptors
const RX_BUF_SIZE: usize = 2048; // RX buffer size (must be >= 1536)
const TX_TIMEOUT_MS: usize = 6000; // TX timeout in milliseconds
const PAGE_SIZE: usize = 4096;

/// RTL8169 Network Interface Controller
pub struct Rtl8169Nic {
    /// MMIO base virtual address
    base_vaddr: usize,

    /// Register accessor
    regs: Rtl8169Regs,

    /// Chip information
    chip_idx: usize,

    /// MAC address
    mac_addr: [u8; 6],

    /// TX descriptor ring
    tx_desc_ring: &'static mut [TxDesc],
    tx_desc_paddr: usize,
    tx_desc_pages: usize,

    /// TX buffers (static allocation)
    tx_buffers: Vec<Option<NetBufBox>>,
    tx_head: usize, // Next descriptor to use
    tx_tail: usize, // Next descriptor to reclaim

    /// RX descriptor ring
    rx_desc_ring: &'static mut [RxDesc],
    rx_desc_paddr: usize,
    rx_desc_pages: usize,

    /// RX buffer pool (wrapped in Arc for alloc_boxed)
    rx_buf_pool: Arc<NetBufPool>,
    rx_buffers: Vec<Option<NetBufBox>>,
    rx_cur: usize, // Current RX descriptor

    /// Link status
    link_up: AtomicBool,
}

// Safety: RTL8169Nic uses proper synchronization for shared state
unsafe impl Send for Rtl8169Nic {}
unsafe impl Sync for Rtl8169Nic {}

impl Rtl8169Nic {
    /// Initialize RTL8169 driver from MMIO base address
    ///
    /// # Arguments
    /// * `base_vaddr` - Virtual address of MMIO region (from PCI BAR)
    /// * `_size` - Size of MMIO region
    ///
    /// # Returns
    /// * `Ok(Rtl8169Nic)` - Successfully initialized driver
    /// * `Err(DevError)` - Initialization failed
    pub fn init(base_vaddr: usize, _size: usize) -> DevResult<Self> {
        info!("RTL8169: Initializing driver at base={:#x}", base_vaddr);

        if base_vaddr == 0 {
            error!("RTL8169: Invalid base address");
            return Err(DevError::InvalidParam);
        }

        let regs = Rtl8169Regs::new(base_vaddr);

        // Step 1: Software reset
        info!("RTL8169: Performing software reset");
        regs.write8(Reg::ChipCmd, chip_cmd::CMD_RESET);

        // Wait for reset to complete
        let mut timeout = 1000;
        while timeout > 0 {
            if (regs.read8(Reg::ChipCmd) & chip_cmd::CMD_RESET) == 0 {
                break;
            }
            timeout -= 1;
        }

        if timeout == 0 {
            error!("RTL8169: Software reset timeout");
            return Err(DevError::BadState);
        }

        info!("RTL8169: Software reset completed");

        // Step 2: Identify chip version
        let tx_config = regs.read32(Reg::TxConfig);
        let version = (((tx_config & 0x7c000000) + ((tx_config & 0x00800000) << 2)) >> 24) as u8;

        let chip_idx = CHIP_VERSIONS
            .iter()
            .position(|c| c.version == version)
            .unwrap_or(0); // Default to first entry if unknown

        let chip_info = &CHIP_VERSIONS[chip_idx];
        info!(
            "RTL8169: Detected chip: {} (version={:#x})",
            chip_info.name, chip_info.version
        );

        // Step 3: Read MAC address
        let mac_low = regs.read32(Reg::Mac0);
        let mac_high = regs.read16(Reg::Mac4);
        let mac_addr = [
            (mac_low & 0xFF) as u8,
            ((mac_low >> 8) & 0xFF) as u8,
            ((mac_low >> 16) & 0xFF) as u8,
            ((mac_low >> 24) & 0xFF) as u8,
            (mac_high & 0xFF) as u8,
            ((mac_high >> 8) & 0xFF) as u8,
        ];

        info!(
            "RTL8169: MAC address: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            mac_addr[0], mac_addr[1], mac_addr[2], mac_addr[3], mac_addr[4], mac_addr[5]
        );

        // Step 4: Allocate TX descriptor ring
        let tx_desc_size = core::mem::size_of::<TxDesc>() * NUM_TX_DESC;
        let tx_desc_pages = (tx_desc_size + PAGE_SIZE - 1) / PAGE_SIZE;
        let (tx_desc_vaddr, tx_desc_paddr) = Self::alloc_dma_pages(tx_desc_pages)?;

        let tx_desc_ring =
            unsafe { core::slice::from_raw_parts_mut(tx_desc_vaddr as *mut TxDesc, NUM_TX_DESC) };

        info!(
            "RTL8169: TX ring: vaddr={:#x}, paddr={:#x}, count={}",
            tx_desc_vaddr, tx_desc_paddr, NUM_TX_DESC
        );

        // Step 5: Allocate RX descriptor ring
        let rx_desc_size = core::mem::size_of::<RxDesc>() * NUM_RX_DESC;
        let rx_desc_pages = (rx_desc_size + PAGE_SIZE - 1) / PAGE_SIZE;
        let (rx_desc_vaddr, rx_desc_paddr) = Self::alloc_dma_pages(rx_desc_pages)?;

        let rx_desc_ring =
            unsafe { core::slice::from_raw_parts_mut(rx_desc_vaddr as *mut RxDesc, NUM_RX_DESC) };

        info!(
            "RTL8169: RX ring: vaddr={:#x}, paddr={:#x}, count={}",
            rx_desc_vaddr, rx_desc_paddr, NUM_RX_DESC
        );

        // Step 6: Create RX buffer pool (already returns Arc<NetBufPool>)
        let rx_buf_pool = NetBufPool::new(NUM_RX_DESC * 2, RX_BUF_SIZE)?;

        // Step 7: Initialize TX descriptors (ring mode)
        for i in 0..NUM_TX_DESC {
            tx_desc_ring[i] = TxDesc::new();
        }

        // Step 8: Initialize RX descriptors and allocate buffers
        let mut rx_buffers = Vec::with_capacity(NUM_RX_DESC);
        for i in 0..NUM_RX_DESC {
            let buf = rx_buf_pool.alloc_boxed().ok_or(DevError::NoMemory)?;
            let buf_paddr = Self::virt_to_phys(buf.raw_buf().as_ptr() as usize) as u32;

            let is_last = i == NUM_RX_DESC - 1;
            rx_desc_ring[i].setup_rx(buf_paddr, RX_BUF_SIZE as u32, is_last);
            rx_buffers.push(Some(buf));
        }

        info!("RTL8169: Descriptors initialized");

        // Step 9: Configure hardware
        Self::configure_hardware(&regs, tx_desc_paddr, rx_desc_paddr, chip_idx)?;

        // Step 10: PHY initialization and auto-negotiation
        let phy = PhyManager::new(&regs);
        phy.init_and_negotiate().map_err(|_| DevError::Io)?;

        let link_up = phy.is_link_up();

        info!("RTL8169: Initialization complete");

        Ok(Self {
            base_vaddr,
            regs,
            chip_idx,
            mac_addr,
            tx_desc_ring,
            tx_desc_paddr,
            tx_desc_pages,
            tx_buffers: {
                let mut v = Vec::with_capacity(NUM_TX_DESC);
                v.resize_with(NUM_TX_DESC, || None);
                v
            },
            tx_head: 0,
            tx_tail: 0,
            rx_desc_ring,
            rx_desc_paddr,
            rx_desc_pages,
            rx_buf_pool,
            rx_buffers,
            rx_cur: 0,
            link_up: AtomicBool::new(link_up),
        })
    }

    /// Allocate DMA-coherent memory pages
    fn alloc_dma_pages(pages: usize) -> DevResult<(usize, usize)> {
        let (vaddr, paddr) =
            crate_interface::call_interface!(KernelFunc::dma_alloc_coherent(pages));
        if vaddr == 0 || paddr == 0 {
            error!("RTL8169: Failed to allocate {} DMA pages", pages);
            return Err(DevError::NoMemory);
        }
        Ok((vaddr, paddr))
    }

    /// Virtual to physical address translation
    fn virt_to_phys(vaddr: usize) -> usize {
        crate_interface::call_interface!(KernelFunc::virt_to_phys(vaddr))
    }

    /// Free DMA pages
    fn free_dma_pages(vaddr: usize, pages: usize) {
        crate_interface::call_interface!(KernelFunc::dma_free_coherent(vaddr, pages));
    }

    /// Configure hardware registers
    fn configure_hardware(
        regs: &Rtl8169Regs,
        tx_desc_paddr: usize,
        rx_desc_paddr: usize,
        chip_idx: usize,
    ) -> DevResult {
        // Unlock configuration registers
        regs.write8(Reg::Cfg9346, cfg9346::UNLOCK);

        // Set TX descriptor base address
        regs.write32(Reg::TxDescStartAddrLow, tx_desc_paddr as u32);
        regs.write32(Reg::TxDescStartAddrHigh, 0);

        // Set RX descriptor base address
        regs.write32(Reg::RxDescStartAddrLow, rx_desc_paddr as u32);
        regs.write32(Reg::RxDescStartAddrHigh, 0);

        // Set early TX threshold
        regs.write8(Reg::EarlyTxThres, EARLY_TX_THLD);

        // Set RX max size
        regs.write16(Reg::RxMaxSize, RX_PACKET_MAX_SIZE);

        // Configure RX: FIFO threshold, DMA burst, accept all
        let rx_config = (RX_FIFO_THRESH << RX_CFG_FIFO_SHIFT)
            | (RX_DMA_BURST << RX_CFG_DMA_SHIFT)
            | rx_mode::ACCEPT_BROADCAST
            | rx_mode::ACCEPT_MULTICAST
            | rx_mode::ACCEPT_MY_PHYS;

        let chip_info = &CHIP_VERSIONS[chip_idx];
        let rx_config = (rx_config & chip_info.rx_config_mask)
            | (regs.read32(Reg::RxConfig) & !chip_info.rx_config_mask);

        regs.write32(Reg::RxConfig, rx_config);

        // Configure TX: DMA burst, inter-frame gap
        let tx_config =
            (TX_DMA_BURST << TX_DMA_SHIFT) | (INTER_FRAME_GAP << TX_INTERFRAME_GAP_SHIFT);
        regs.write32(Reg::TxConfig, tx_config);

        // Clear RX missed packet counter
        regs.write32(Reg::RxMissed, 0);

        // Enable TX and RX (for older chips, do this before lock)
        if chip_idx <= 5 {
            regs.write8(Reg::ChipCmd, chip_cmd::CMD_TX_ENB | chip_cmd::CMD_RX_ENB);
        }

        // Lock configuration registers
        regs.write8(Reg::Cfg9346, cfg9346::LOCK);

        // Disable interrupts (polling mode)
        regs.write16(Reg::IntrMask, 0);

        // Clear any pending interrupts
        regs.write16(Reg::IntrStatus, 0xFFFF);

        // Enable TX and RX (for newer chips, do this after lock)
        if chip_idx > 5 {
            regs.write8(Reg::ChipCmd, chip_cmd::CMD_TX_ENB | chip_cmd::CMD_RX_ENB);
        }

        info!("RTL8169: Hardware configured");
        Ok(())
    }

    /// Reclaim completed TX descriptors
    fn reclaim_tx_buffers(&mut self) {
        while self.tx_tail != self.tx_head {
            let desc = &self.tx_desc_ring[self.tx_tail];
            if desc.is_owned() {
                break; // Still owned by hardware
            }

            // Descriptor completed, release buffer
            self.tx_buffers[self.tx_tail] = None;
            self.tx_tail = (self.tx_tail + 1) % NUM_TX_DESC;
        }
    }
}

impl Drop for Rtl8169Nic {
    fn drop(&mut self) {
        info!("RTL8169: Shutting down");

        // Stop TX/RX
        self.regs.write8(Reg::ChipCmd, 0);

        // Disable interrupts
        self.regs.write16(Reg::IntrMask, 0);

        // Free DMA memory
        Self::free_dma_pages(self.tx_desc_ring.as_ptr() as usize, self.tx_desc_pages);
        Self::free_dma_pages(self.rx_desc_ring.as_ptr() as usize, self.rx_desc_pages);
    }
}

impl BaseDriverOps for Rtl8169Nic {
    fn device_name(&self) -> &str {
        "rtl8169"
    }

    fn device_type(&self) -> DeviceType {
        DeviceType::Net
    }
}

impl NetDriverOps for Rtl8169Nic {
    fn mac_address(&self) -> EthernetAddress {
        EthernetAddress(self.mac_addr)
    }

    fn can_transmit(&self) -> bool {
        // Check if there's space in TX ring
        let next_head = (self.tx_head + 1) % NUM_TX_DESC;
        if next_head == self.tx_tail {
            return false; // Ring full
        }

        // Check if current descriptor is free
        !self.tx_desc_ring[self.tx_head].is_owned()
    }

    fn can_receive(&self) -> bool {
        // Check if current RX descriptor has data
        !self.rx_desc_ring[self.rx_cur].is_owned()
    }

    fn rx_queue_size(&self) -> usize {
        NUM_RX_DESC
    }

    fn tx_queue_size(&self) -> usize {
        NUM_TX_DESC
    }

    fn recycle_rx_buffer(&mut self, rx_buf: NetBufPtr) -> DevResult {
        // Convert back to NetBuf and drop to return to pool
        let buf = unsafe { NetBuf::from_buf_ptr(rx_buf) };
        drop(buf);
        Ok(())
    }

    fn recycle_tx_buffers(&mut self) -> DevResult {
        // Reclaim completed TX descriptors
        self.reclaim_tx_buffers();
        Ok(())
    }

    fn transmit(&mut self, tx_buf: NetBufPtr) -> DevResult {
        // Reclaim completed TX buffers first
        self.reclaim_tx_buffers();

        if !self.can_transmit() {
            return Err(DevError::Again);
        }

        let buf = unsafe { NetBuf::from_buf_ptr(tx_buf) };
        let pkt = buf.packet();
        let len = pkt.len();

        if len == 0 || len > RX_BUF_SIZE {
            error!("RTL8169: Invalid TX length: {}", len);
            return Err(DevError::InvalidParam);
        }

        // Get physical address of buffer
        let buf_paddr = Self::virt_to_phys(pkt.as_ptr() as usize) as u32;

        // Setup TX descriptor
        let idx = self.tx_head;
        let is_last = idx == NUM_TX_DESC - 1;
        self.tx_desc_ring[idx].setup_tx(buf_paddr, len as u32, is_last);

        // Store buffer reference (buf is already Box<NetBuf>)
        self.tx_buffers[idx] = Some(buf); // Advance head
        self.tx_head = (self.tx_head + 1) % NUM_TX_DESC;

        // Trigger TX by writing TxPoll register
        self.regs.write8(Reg::TxPoll, 0x40);

        // Wait for transmission to complete (polling mode)
        let start = Self::get_ticks();
        while self.tx_desc_ring[idx].is_owned() {
            if Self::get_ticks() - start > TX_TIMEOUT_MS {
                error!("RTL8169: TX timeout");
                return Err(DevError::Io);
            }
        }

        Ok(())
    }

    fn receive(&mut self) -> DevResult<NetBufPtr> {
        if !self.can_receive() {
            return Err(DevError::Again);
        }

        let idx = self.rx_cur;
        let desc = &self.rx_desc_ring[idx];

        // Get received length
        let len = desc.get_rx_length().ok_or(DevError::Io)?;

        if len == 0 {
            return Err(DevError::Again);
        }

        // Take the buffer
        let mut rx_buf = self.rx_buffers[idx].take().ok_or(DevError::BadState)?;

        // Set packet length
        rx_buf.set_packet_len(len);

        // Allocate new buffer for this descriptor
        let new_buf = self.rx_buf_pool.alloc_boxed().ok_or(DevError::NoMemory)?;
        let new_buf_paddr = Self::virt_to_phys(new_buf.raw_buf().as_ptr() as usize) as u32;

        // Setup descriptor for next receive
        let is_last = idx == NUM_RX_DESC - 1;
        self.rx_desc_ring[idx].setup_rx(new_buf_paddr, RX_BUF_SIZE as u32, is_last);
        self.rx_buffers[idx] = Some(new_buf);

        // Advance to next descriptor
        self.rx_cur = (self.rx_cur + 1) % NUM_RX_DESC;

        Ok(rx_buf.into_buf_ptr())
    }

    fn alloc_tx_buffer(&mut self, size: usize) -> DevResult<NetBufPtr> {
        // Allocate from RX pool (they share the same pool)
        let mut buf = self.rx_buf_pool.alloc_boxed().ok_or(DevError::NoMemory)?;
        buf.set_packet_len(size);
        Ok(buf.into_buf_ptr())
    }
}

impl Rtl8169Nic {
    /// Get current tick count (simplified - just use spin loop counter)
    fn get_ticks() -> usize {
        // Simple counter-based timeout
        // In real implementation, should use platform timer
        static mut COUNTER: usize = 0;
        unsafe {
            COUNTER = COUNTER.wrapping_add(1);
            COUNTER
        }
    }
}
