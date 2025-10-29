//! GMAC (Gigabit MAC) driver for RK3588 SoC
//!
//! This driver implements the network device interface for the Synopsys
//! DesignWare Ethernet MAC 4.20a controller used in Rockchip RK3588 SoC.
//!
//! # Features
//! - Single queue mode (Queue 0)
//! - Polling mode (interrupt support reserved for future)
//! - NetBufPool for zero-copy buffer management
//! - Chained descriptor mode
//! - Basic Ethernet functionality (MAC address, TX/RX)
//!
//! # Architecture
//! ```text
//! Application
//!     ↓
//! NetDriverOps trait
//!     ↓
//! GmacNic (this module)
//!     ↓
//! GmacRegs (register access)
//!     ↓
//! KernelFunc trait (platform abstraction)
//!     ↓
//! Hardware (MMIO)
//! ```

extern crate alloc;

mod desc;
mod regs;

// re-exported utilities used locally
use alloc::vec::Vec;
use alloc::{collections::VecDeque, sync::Arc};
use core::sync::atomic::AtomicBool;

use axdriver_base::{BaseDriverOps, DevError, DevResult, DeviceType};
use desc::{RxDesc, TxDesc};
use log::*;
use regs::*;

use crate::{EthernetAddress, NetBuf, NetBufBox, NetBufPool, NetBufPtr, NetDriverOps};

/// Platform-specific kernel functions trait
///
/// This trait must be implemented by the platform integration layer
/// to provide memory management and address translation services.
#[crate_interface::def_interface]
pub trait KernelFunc {
    /// Convert virtual address to physical address
    fn virt_to_phys(addr: usize) -> usize;

    /// Convert physical address to virtual address
    fn phys_to_virt(addr: usize) -> usize;

    /// Allocate DMA-coherent memory (page-aligned)
    ///
    /// Returns (virtual_address, physical_address) tuple.
    /// Returns (0, 0) on failure.
    fn dma_alloc_coherent(pages: usize) -> (usize, usize);

    /// Free DMA-coherent memory
    fn dma_free_coherent(vaddr: usize, pages: usize);

    /// Request interrupt handler registration (reserved for future use)
    ///
    /// Current implementation uses polling mode, so this has a default
    /// no-op implementation.
    #[allow(unused_variables)]
    fn dma_request_irq(irq: usize, handler: fn()) {
        // Default implementation: do nothing (polling mode)
    }
}

/// Configuration constants
const RX_RING_SIZE: usize = 128; // Number of RX descriptors
const TX_RING_SIZE: usize = 128; // Number of TX descriptors
const RX_BUFFER_SIZE: usize = 1526; // MTU(1500) + Ethernet header(14) + VLAN(4) + CRC(4) + align(4)
const RX_POOL_SIZE: usize = 256; // Buffer pool size (2x ring size)
const PAGE_SIZE: usize = 4096; // Page size for DMA allocation

/// GMAC Network Interface Controller driver
///
/// Implements the NetDriverOps trait for RK3588 GMAC controller.
/// Uses polling mode with reserved support for future interrupt implementation.
pub struct GmacNic {
    /// MMIO base virtual address
    base_vaddr: usize,

    /// Cached MAC address
    mac_addr: [u8; 6],

    /// TX descriptor ring
    tx_desc_ring: &'static mut [TxDesc],
    /// TX descriptor physical address (for cleanup)
    _tx_desc_paddr: usize,
    /// TX descriptor pages count (for cleanup)
    tx_desc_pages: usize,
    /// TX buffer tracking (for reclaim)
    tx_buffers: Vec<Option<NetBufBox>>,
    /// TX ring head index (next descriptor to use)
    tx_head: usize,
    /// TX ring tail index (next descriptor to reclaim)
    tx_tail: usize,

    /// RX descriptor ring
    rx_desc_ring: &'static mut [RxDesc],
    /// RX descriptor physical address (for cleanup)
    _rx_desc_paddr: usize,
    /// RX descriptor pages count (for cleanup)
    rx_desc_pages: usize,
    /// RX buffer pool for zero-copy operation
    rx_buf_pool: Arc<NetBufPool>,
    /// RX buffers currently in descriptor ring
    rx_buffers: Vec<Option<NetBufBox>>,
    /// RX ring current index
    rx_cur: usize,
    /// Received packet queue (for batching)
    rx_buffer_queue: VecDeque<NetBufPtr>,

    /// Interrupt mode enabled flag (reserved for future)
    #[allow(dead_code)]
    irq_enabled: bool,
    /// RX interrupt pending flag (reserved for future)
    #[allow(dead_code)]
    pending_rx: AtomicBool,
    /// TX interrupt pending flag (reserved for future)
    #[allow(dead_code)]
    pending_tx: AtomicBool,
}

// For compatibility with other drivers in this tree we mark the NIC as
// Send + Sync similarly to `FXmacNic` which also used `unsafe impl` in this
// repository. The safety assumption here is that all internal mutations are
// properly synchronized by the upper layers (or performed in a single-thread
// context). This is a pragmatic choice for the initial driver implementation.
unsafe impl Sync for GmacNic {}
unsafe impl Send for GmacNic {}

impl GmacNic {
    /// Initialize GMAC driver
    ///
    /// # Arguments
    /// * `base_vaddr` - MMIO base virtual address
    /// * `_size` - MMIO region size (currently unused but reserved)
    ///
    /// # Returns
    /// * `Ok(GmacNic)` - Successfully initialized driver instance
    /// * `Err(DevError)` - Initialization failed
    pub fn init(base_vaddr: usize, _size: usize) -> DevResult<Self> {
        info!("GMAC: Initializing driver at base={:#x}", base_vaddr);

        // Validate base address
        if base_vaddr == 0 {
            error!("GMAC: Invalid base address");
            return Err(DevError::InvalidParam);
        }

        let regs = GmacRegs::new(base_vaddr);

        // Step 1: Software reset
        info!("GMAC: Performing software reset");
        regs.write_reg(DMA_BUS_MODE, DMA_BUS_MODE_SWR);

        // Wait for reset to complete (bit clears automatically)
        let mut timeout = 1000;
        while timeout > 0 {
            if (regs.read_reg(DMA_BUS_MODE) & DMA_BUS_MODE_SWR) == 0 {
                break;
            }
            timeout -= 1;
        }

        if timeout == 0 {
            error!("GMAC: Software reset timeout");
            return Err(DevError::BadState);
        }

        info!("GMAC: Software reset completed");

        // Step 2: Read MAC address from hardware
        let mac_low = regs.read_reg(MAC_ADDRESS0_LOW);
        let mac_high = regs.read_reg(MAC_ADDRESS0_HIGH);
        let mac_addr = [
            (mac_low & 0xFF) as u8,
            ((mac_low >> 8) & 0xFF) as u8,
            ((mac_low >> 16) & 0xFF) as u8,
            ((mac_low >> 24) & 0xFF) as u8,
            (mac_high & 0xFF) as u8,
            ((mac_high >> 8) & 0xFF) as u8,
        ];

        // Validate MAC address
        if mac_addr == [0, 0, 0, 0, 0, 0] || mac_addr == [0xFF; 6] {
            warn!("GMAC: Invalid MAC address from hardware, using default");
            // Use a locally administered address
            let mac_addr = [0x02, 0x00, 0x00, 0x00, 0x00, 0x01];
            regs.write_reg(
                MAC_ADDRESS0_LOW,
                (mac_addr[0] as u32)
                    | ((mac_addr[1] as u32) << 8)
                    | ((mac_addr[2] as u32) << 16)
                    | ((mac_addr[3] as u32) << 24),
            );
            regs.write_reg(
                MAC_ADDRESS0_HIGH,
                (mac_addr[4] as u32) | ((mac_addr[5] as u32) << 8),
            );
        }

        info!(
            "GMAC: MAC address: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            mac_addr[0], mac_addr[1], mac_addr[2], mac_addr[3], mac_addr[4], mac_addr[5]
        );

        // Step 3: Allocate TX descriptor ring
        let tx_desc_size = core::mem::size_of::<TxDesc>() * TX_RING_SIZE;
        let tx_desc_pages = (tx_desc_size + PAGE_SIZE - 1) / PAGE_SIZE;
        let (tx_desc_vaddr, tx_desc_paddr) =
            crate_interface::call_interface!(KernelFunc::dma_alloc_coherent(tx_desc_pages));

        if tx_desc_vaddr == 0 || tx_desc_paddr == 0 {
            error!("GMAC: Failed to allocate TX descriptor ring");
            return Err(DevError::NoMemory);
        }

        let tx_desc_ring =
            unsafe { core::slice::from_raw_parts_mut(tx_desc_vaddr as *mut TxDesc, TX_RING_SIZE) };

        info!(
            "GMAC: TX ring allocated: vaddr={:#x}, paddr={:#x}, size={}",
            tx_desc_vaddr, tx_desc_paddr, TX_RING_SIZE
        );

        // Step 4: Allocate RX descriptor ring
        let rx_desc_size = core::mem::size_of::<RxDesc>() * RX_RING_SIZE;
        let rx_desc_pages = (rx_desc_size + PAGE_SIZE - 1) / PAGE_SIZE;
        let (rx_desc_vaddr, rx_desc_paddr) =
            crate_interface::call_interface!(KernelFunc::dma_alloc_coherent(rx_desc_pages));

        if rx_desc_vaddr == 0 || rx_desc_paddr == 0 {
            error!("GMAC: Failed to allocate RX descriptor ring");
            crate_interface::call_interface!(KernelFunc::dma_free_coherent(
                tx_desc_vaddr,
                tx_desc_pages
            ));
            return Err(DevError::NoMemory);
        }

        let rx_desc_ring =
            unsafe { core::slice::from_raw_parts_mut(rx_desc_vaddr as *mut RxDesc, RX_RING_SIZE) };

        info!(
            "GMAC: RX ring allocated: vaddr={:#x}, paddr={:#x}, size={}",
            rx_desc_vaddr, rx_desc_paddr, RX_RING_SIZE
        );

        // Step 5: Create RX buffer pool
        let rx_buf_pool = NetBufPool::new(RX_POOL_SIZE, RX_BUFFER_SIZE)?;
        info!("GMAC: RX buffer pool created: size={}", RX_POOL_SIZE);

        // Step 6: Initialize TX descriptors (chained mode)
        for i in 0..TX_RING_SIZE {
            let next_desc_paddr = if i == TX_RING_SIZE - 1 {
                tx_desc_paddr as u32 // Wrap to first descriptor
            } else {
                (tx_desc_paddr + (i + 1) * core::mem::size_of::<TxDesc>()) as u32
            };

            tx_desc_ring[i] = TxDesc::new();
            tx_desc_ring[i].set_next_desc(next_desc_paddr);
            tx_desc_ring[i].tdes0 = TxDesc::TCH; // Chained mode
        }

        debug!("GMAC: TX descriptors initialized (chained mode)");

        // Step 7: Initialize RX descriptors (chained mode, pre-allocate buffers)
        let mut rx_buffers = Vec::with_capacity(RX_RING_SIZE);
        for i in 0..RX_RING_SIZE {
            let buf = rx_buf_pool.alloc_boxed().ok_or(DevError::NoMemory)?;
            let buf_paddr = crate_interface::call_interface!(KernelFunc::virt_to_phys(
                buf.raw_buf().as_ptr() as usize
            )) as u32;

            let next_desc_paddr = if i == RX_RING_SIZE - 1 {
                rx_desc_paddr as u32 // Wrap to first descriptor
            } else {
                (rx_desc_paddr + (i + 1) * core::mem::size_of::<RxDesc>()) as u32
            };

            rx_desc_ring[i].setup_rx(buf_paddr, RX_BUFFER_SIZE as u32, next_desc_paddr);
            rx_buffers.push(Some(buf));
        }

        debug!("GMAC: RX descriptors initialized (chained mode, pre-allocated buffers)");

        // Step 8: Configure DMA
        regs.write_reg(DMA_TX_DESC_LIST_ADDR, tx_desc_paddr as u32);
        regs.write_reg(DMA_RX_DESC_LIST_ADDR, rx_desc_paddr as u32);

        // Configure bus mode: Fixed burst, address-aligned beats
        regs.write_reg(
            DMA_BUS_MODE,
            DMA_BUS_MODE_PBL_8 | DMA_BUS_MODE_FB | DMA_BUS_MODE_AAL | DMA_BUS_MODE_USP,
        );

        // Configure operation mode: Store and forward for both TX and RX
        regs.write_reg(
            DMA_OPERATION_MODE,
            DMA_OP_MODE_TSF | DMA_OP_MODE_RSF | DMA_OP_MODE_FUF,
        );

        debug!("GMAC: DMA configured");

        // Step 9: Configure MAC
        // Enable transmitter and receiver, full duplex, 1000Mbps
        regs.write_reg(
            MAC_CONFIGURATION,
            MAC_CONFIG_TE
                | MAC_CONFIG_RE
                | MAC_CONFIG_DM
                | MAC_CONFIG_IPC
                | MAC_CONFIG_ACS
                | MAC_CONFIG_DO,
        );

        // Configure frame filter: Receive all for now (promiscuous mode)
        regs.write_reg(MAC_FRAME_FILTER, MAC_FILTER_RA);

        debug!("GMAC: MAC configured");

        // Step 10: Disable interrupts (polling mode)
        regs.write_reg(DMA_INTERRUPT_ENABLE, 0);
        debug!("GMAC: Interrupts disabled (polling mode)");

        // Step 11: Start DMA
        regs.set_bits(DMA_OPERATION_MODE, DMA_OP_MODE_ST | DMA_OP_MODE_SR);
        info!("GMAC: DMA started (TX and RX enabled)");

        // Clear any pending status
        let status = regs.read_and_clear(DMA_STATUS);
        debug!("GMAC: Initial DMA status: {:#x}", status);

        Ok(Self {
            base_vaddr,
            mac_addr,
            tx_desc_ring,
            _tx_desc_paddr: tx_desc_paddr,
            tx_desc_pages,
            tx_buffers: {
                let mut tb = Vec::with_capacity(TX_RING_SIZE);
                tb.resize_with(TX_RING_SIZE, || None);
                tb
            },
            tx_head: 0,
            tx_tail: 0,
            rx_desc_ring,
            _rx_desc_paddr: rx_desc_paddr,
            rx_desc_pages,
            rx_buf_pool,
            rx_buffers,
            rx_cur: 0,
            rx_buffer_queue: VecDeque::with_capacity(RX_RING_SIZE),
            irq_enabled: false,
            pending_rx: AtomicBool::new(false),
            pending_tx: AtomicBool::new(false),
        })
    }

    /// Enable interrupt mode (reserved for future implementation)
    #[allow(dead_code)]
    fn enable_interrupts(&mut self, _rx_irq: usize, _tx_irq: usize) -> DevResult {
        warn!("GMAC: enable_interrupts called but not implemented (polling mode)");
        Err(DevError::Unsupported)
    }

    /// RX interrupt handler (reserved for future implementation)
    #[allow(dead_code)]
    fn rx_interrupt_handler() {
        warn!("GMAC: rx_interrupt_handler called but not implemented");
    }

    /// TX interrupt handler (reserved for future implementation)
    #[allow(dead_code)]
    fn tx_interrupt_handler() {
        warn!("GMAC: tx_interrupt_handler called but not implemented");
    }

    /// Check and clear interrupt status (reserved for future)
    #[allow(dead_code)]
    fn check_and_clear_interrupts(&mut self) -> u32 {
        let regs = GmacRegs::new(self.base_vaddr);
        regs.read_and_clear(DMA_STATUS)
    }
}

impl BaseDriverOps for GmacNic {
    fn device_name(&self) -> &str {
        "gmac"
    }

    fn device_type(&self) -> DeviceType {
        DeviceType::Net
    }

    fn irq_number(&self) -> Option<u32> {
        None
    }
}

impl NetDriverOps for GmacNic {
    fn mac_address(&self) -> EthernetAddress {
        EthernetAddress(self.mac_addr)
    }

    fn can_transmit(&self) -> bool {
        // Check if TX ring has free space
        let next_head = (self.tx_head + 1) % TX_RING_SIZE;
        if next_head == self.tx_tail {
            return false; // Ring full
        }

        // Check if current descriptor is free (OWN=0)
        !self.tx_desc_ring[self.tx_head].is_owned_by_dma()
    }

    fn can_receive(&self) -> bool {
        // Check local queue first
        if !self.rx_buffer_queue.is_empty() {
            return true;
        }

        // Check if current RX descriptor has been updated by DMA
        !self.rx_desc_ring[self.rx_cur].is_owned_by_dma()
    }

    fn rx_queue_size(&self) -> usize {
        RX_RING_SIZE
    }

    fn tx_queue_size(&self) -> usize {
        TX_RING_SIZE
    }

    fn recycle_rx_buffer(&mut self, rx_buf: NetBufPtr) -> DevResult {
        // Convert back to NetBuf and let it drop (returns to pool automatically)
        let buf = unsafe { NetBuf::from_buf_ptr(rx_buf) };
        drop(buf);
        Ok(())
    }

    fn recycle_tx_buffers(&mut self) -> DevResult {
        // Scan TX ring and reclaim completed buffers
        let mut reclaimed = 0;
        while self.tx_tail != self.tx_head {
            let desc = &self.tx_desc_ring[self.tx_tail];

            if desc.is_owned_by_dma() {
                break; // Still owned by DMA
            }

            // Free the buffer
            self.tx_buffers[self.tx_tail] = None;

            // Advance tail
            self.tx_tail = (self.tx_tail + 1) % TX_RING_SIZE;
            reclaimed += 1;
        }

        if reclaimed > 0 {
            trace!("GMAC: Reclaimed {} TX buffers", reclaimed);
        }

        Ok(())
    }

    fn transmit(&mut self, tx_buf: NetBufPtr) -> DevResult {
        if !self.can_transmit() {
            return Err(DevError::Again);
        }

        let desc = &mut self.tx_desc_ring[self.tx_head];

        // Get buffer info
        let buf_ptr = tx_buf.packet().as_ptr();
        let buf_len = tx_buf.packet_len();
        let buf_paddr =
            crate_interface::call_interface!(KernelFunc::virt_to_phys(buf_ptr as usize)) as u32;

        trace!(
            "GMAC: TX packet: idx={}, len={}, paddr={:#x}",
            self.tx_head, buf_len, buf_paddr
        );

        // Setup descriptor for transmission
        desc.set_buf1_addr(buf_paddr);
        desc.set_buf1_size(buf_len as u32);
        desc.tdes0 = TxDesc::OWN | TxDesc::FS | TxDesc::LS | TxDesc::TCH;
        // Note: TxDesc::IC (Interrupt on Completion) is NOT set in polling mode

        // Store buffer for later reclaim
        let tx_buf_box = unsafe { NetBuf::from_buf_ptr(tx_buf) };
        self.tx_buffers[self.tx_head] = Some(tx_buf_box);

        // Advance head
        self.tx_head = (self.tx_head + 1) % TX_RING_SIZE;

        // Notify DMA (write any value)
        let regs = GmacRegs::new(self.base_vaddr);
        regs.write_reg(DMA_TX_POLL_DEMAND, 1);

        Ok(())
    }

    fn receive(&mut self) -> DevResult<NetBufPtr> {
        // Check local queue first (batched packets)
        if let Some(buf) = self.rx_buffer_queue.pop_front() {
            return Ok(buf);
        }

        // Poll hardware descriptor
        let desc = &mut self.rx_desc_ring[self.rx_cur];

        if desc.is_owned_by_dma() {
            return Err(DevError::Again); // No packet available
        }

        // Check for errors
        if desc.has_error() {
            warn!(
                "GMAC: RX error at idx={}, status={:#x}",
                self.rx_cur, desc.rdes0
            );
            // Recycle descriptor
            desc.set_owned_by_dma();
            self.rx_cur = (self.rx_cur + 1) % RX_RING_SIZE;

            // Notify DMA
            let regs = GmacRegs::new(self.base_vaddr);
            regs.write_reg(DMA_RX_POLL_DEMAND, 1);

            return Err(DevError::BadState);
        }

        // Check if complete frame
        if !desc.is_complete_frame() {
            error!("GMAC: Fragmented frame not supported");
            desc.set_owned_by_dma();
            self.rx_cur = (self.rx_cur + 1) % RX_RING_SIZE;
            return Err(DevError::BadState);
        }

        // Extract frame length (includes CRC, need to subtract 4 bytes)
        let frame_len = desc.frame_length();
        let packet_len = if frame_len >= 4 {
            frame_len - 4 // Remove CRC
        } else {
            frame_len
        };

        trace!("GMAC: RX packet: idx={}, len={}", self.rx_cur, packet_len);

        // Get old buffer
        let mut old_buf = self.rx_buffers[self.rx_cur].take().unwrap();

        // Allocate new buffer for this descriptor
        let new_buf = self.rx_buf_pool.alloc_boxed().ok_or(DevError::NoMemory)?;
        let new_buf_paddr = crate_interface::call_interface!(KernelFunc::virt_to_phys(
            new_buf.raw_buf().as_ptr() as usize
        )) as u32;

        // Get next descriptor address from current descriptor
        let next_desc = desc.rdes3;

        // Setup descriptor with new buffer
        desc.setup_rx(new_buf_paddr, RX_BUFFER_SIZE as u32, next_desc);

        // Store new buffer
        self.rx_buffers[self.rx_cur] = Some(new_buf);

        // Notify DMA
        let regs = GmacRegs::new(self.base_vaddr);
        regs.write_reg(DMA_RX_POLL_DEMAND, 1);

        // Advance index
        self.rx_cur = (self.rx_cur + 1) % RX_RING_SIZE;

        // Prepare old buffer for return
        old_buf.set_packet_len(packet_len);
        Ok(old_buf.into_buf_ptr())
    }

    fn alloc_tx_buffer(&mut self, size: usize) -> DevResult<NetBufPtr> {
        let mut buf = self.rx_buf_pool.alloc_boxed().ok_or(DevError::NoMemory)?;
        buf.set_packet_len(size);
        Ok(buf.into_buf_ptr())
    }
}

impl Drop for GmacNic {
    fn drop(&mut self) {
        info!("GMAC: Dropping driver, cleaning up resources");

        // Stop DMA
        let regs = GmacRegs::new(self.base_vaddr);
        regs.clear_bits(DMA_OPERATION_MODE, DMA_OP_MODE_ST | DMA_OP_MODE_SR);

        // Disable MAC
        regs.clear_bits(MAC_CONFIGURATION, MAC_CONFIG_TE | MAC_CONFIG_RE);

        // Free descriptor rings
        crate_interface::call_interface!(KernelFunc::dma_free_coherent(
            self.tx_desc_ring.as_ptr() as usize,
            self.tx_desc_pages
        ));
        crate_interface::call_interface!(KernelFunc::dma_free_coherent(
            self.rx_desc_ring.as_ptr() as usize,
            self.rx_desc_pages
        ));

        info!("GMAC: Driver cleanup completed");
    }
}
