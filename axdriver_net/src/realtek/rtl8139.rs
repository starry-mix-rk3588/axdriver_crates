//! RTL8139 Fast Ethernet driver implementation

use super::kernel_func::UseKernelFunc as KF;
use super::regs::rtl8139::*;
use crate::{EthernetAddress, NetBufPtr, NetDriverOps};
use axdriver_base::{BaseDriverOps, DevError, DevResult, DeviceType};
use core::ptr::NonNull;

/// RTL8139 constants
pub const NUM_TX_DESC: usize = 4;
pub const TX_BUF_SIZE: usize = 2048;
pub const RX_BUF_SIZE: usize = 8192 + 16 + 1536; // 8KB + margin
const MIN_ETH_FRAME_SIZE: usize = 60;

/// RTL8139 Fast Ethernet Driver
pub struct Rtl8139Driver {
    base_addr: usize,
    irq: u8,
    mac: EthernetAddress,

    // Transmit state
    tx_cur: usize,
    tx_buf_vaddr: [usize; NUM_TX_DESC],
    tx_buf_paddr: [usize; NUM_TX_DESC],

    // Receive state
    rx_buf_vaddr: usize,
    rx_buf_paddr: usize,
    cur_rx: usize,
}

impl Rtl8139Driver {
    /// Create a new RTL8139 driver instance
    pub fn new(base_addr: usize, irq: u8) -> DevResult<Self> {
        Ok(Self {
            base_addr,
            irq,
            mac: EthernetAddress([0; 6]),
            tx_cur: 0,
            tx_buf_vaddr: [0; NUM_TX_DESC],
            tx_buf_paddr: [0; NUM_TX_DESC],
            rx_buf_vaddr: 0,
            rx_buf_paddr: 0,
            cur_rx: 0,
        })
    }

    /// Initialize the RTL8139 hardware
    pub fn init(&mut self) -> DevResult {
        log::info!("[RTL8139] Initializing at {:#x}", self.base_addr);

        // Step 1: Power on the device
        log::debug!("[RTL8139] Step 1: Power on device (CONFIG1 = 0x00)");
        self.write_reg8(CONFIG1, 0x00);

        // Step 2: Software reset
        log::debug!("[RTL8139] Step 2: Performing software reset");
        self.software_reset()?;

        // Step 3: Allocate receive buffer
        log::debug!("[RTL8139] Step 3: Allocating RX buffer (size: {})", RX_BUF_SIZE);
        let rx_pages = (RX_BUF_SIZE + 4095) / 4096;
        let (rx_vaddr, rx_paddr) = KF::dma_alloc_coherent(rx_pages);
        if rx_vaddr == 0 {
            log::error!("[RTL8139] Failed to allocate RX buffer");
            return Err(DevError::NoMemory);
        }
        self.rx_buf_vaddr = rx_vaddr;
        self.rx_buf_paddr = rx_paddr;
        log::debug!("[RTL8139] RX buffer allocated at vaddr={:#x}, paddr={:#x}", rx_vaddr, rx_paddr);

        // Step 4: Allocate transmit buffers
        log::debug!("[RTL8139] Step 4: Allocating {} TX buffers (size: {})", NUM_TX_DESC, TX_BUF_SIZE);
        let tx_pages = (TX_BUF_SIZE + 4095) / 4096;
        for i in 0..NUM_TX_DESC {
            let (tx_vaddr, tx_paddr) = KF::dma_alloc_coherent(tx_pages);
            if tx_vaddr == 0 {
                log::error!("[RTL8139] Failed to allocate TX buffer {}", i);
                return Err(DevError::NoMemory);
            }
            self.tx_buf_vaddr[i] = tx_vaddr;
            self.tx_buf_paddr[i] = tx_paddr;
            log::debug!("[RTL8139] TX buffer {} allocated at vaddr={:#x}, paddr={:#x}", i, tx_vaddr, tx_paddr);
        }

        // Step 5: Enable Tx/Rx (before configuration)
        log::debug!("[RTL8139] Step 5: Enabling Tx/Rx");
        let cmd_val = (self.read_reg8(CR) & !0x1C) | CR_TE | CR_RE;
        self.write_reg8(CR, cmd_val);
        log::debug!("[RTL8139] CMD register = {:#x}", self.read_reg8(CR));

        // Step 6: Configure TCR (Transmit Configuration)
        // TCR: DMA burst size = 1024 bytes (6 << 8), normal IFG (3 << 24)
        // Note: Hardware may set additional version/reserved bits
        let tcr_val = (6 << 8) | (3 << 24);
        log::debug!("[RTL8139] Step 6: Configuring TCR = {:#x}", tcr_val);
        self.write_reg32(TCR, tcr_val);
        let tcr_read = self.read_reg32(TCR);
        log::debug!("[RTL8139] TCR register = {:#x} (hw may set additional bits)", tcr_read);

        // Step 7: Configure RCR (Receive Configuration)
        // RCR: Accept all packets (0xF) + WRAP (1<<7) + 64K buffer (1<<11) + max DMA (7<<8)
        let rcr_val = (1 << 11) | (7 << 8) | (1 << 7) | (1 << 3) | (1 << 2) | (1 << 1) | (1 << 0);
        log::debug!("[RTL8139] Step 7: Configuring RCR = {:#x}", rcr_val);
        self.write_reg32(RCR, rcr_val);
        log::debug!("[RTL8139] RCR register = {:#x}", self.read_reg32(RCR));

        // Step 8: Set transmit addresses in TSAD0-3
        log::debug!("[RTL8139] Step 8: Setting TX descriptor addresses");
        for i in 0..NUM_TX_DESC {
            let tsad_reg = TSAD0 + (i as u16 * 4);
            self.write_reg32(tsad_reg, self.tx_buf_paddr[i] as u32);
            log::debug!("[RTL8139] TSAD{} = {:#x}", i, self.read_reg32(tsad_reg));
        }

        // Step 9: Set receive buffer address (RBSTART)
        log::debug!("[RTL8139] Step 9: Setting RX buffer address");
        self.write_reg32(RBSTART, self.rx_buf_paddr as u32);
        log::debug!("[RTL8139] RBSTART = {:#x}", self.read_reg32(RBSTART));

        // Step 10: Initialize missed packet counter (MPC)
        log::debug!("[RTL8139] Step 10: Initializing MPC");
        self.write_reg32(MPC, 0);
        log::debug!("[RTL8139] MPC = {:#x}", self.read_reg32(MPC));

        // Step 11: Configure interrupt mask
        log::debug!("[RTL8139] Step 11: Configuring interrupts");
        let imr_val = INT_ROK | INT_TOK | INT_RER | INT_TER | INT_RXOVW;
        self.write_reg16(IMR, imr_val);
        log::debug!("[RTL8139] IMR = {:#x}", self.read_reg16(IMR));

        // Step 12: Read MAC address
        log::debug!("[RTL8139] Step 12: Reading MAC address");
        self.read_mac_address();

        log::info!(
            "[RTL8139] Initialized successfully with MAC: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            self.mac.0[0],
            self.mac.0[1],
            self.mac.0[2],
            self.mac.0[3],
            self.mac.0[4],
            self.mac.0[5]
        );

        // Verify critical registers
        log::debug!("[RTL8139] Final register status:");
        log::debug!("[RTL8139]   CR  = {:#x}", self.read_reg8(CR));
        log::debug!("[RTL8139]   TCR = {:#x}", self.read_reg32(TCR));
        log::debug!("[RTL8139]   RCR = {:#x}", self.read_reg32(RCR));
        log::debug!("[RTL8139]   IMR = {:#x}", self.read_reg16(IMR));

        Ok(())
    }

    /// Perform software reset
    fn software_reset(&self) -> DevResult {
        log::debug!("[RTL8139] Initiating software reset (CR_RST)");
        self.write_reg8(CR, CR_RST);

        let mut timeout = 1000;
        while (self.read_reg8(CR) & CR_RST) != 0 {
            if timeout == 0 {
                log::error!("[RTL8139] Reset timeout - CR still has RST bit set");
                return Err(DevError::BadState);
            }
            timeout -= 1;
            KF::busy_wait(core::time::Duration::from_micros(100));
        }

        log::debug!("[RTL8139] Software reset completed successfully");
        Ok(())
    }

    /// Read MAC address from device
    fn read_mac_address(&mut self) {
        let mac_low = self.read_reg32(IDR0);
        let mac_high = self.read_reg16(IDR4);

        self.mac.0[0] = (mac_low & 0xFF) as u8;
        self.mac.0[1] = ((mac_low >> 8) & 0xFF) as u8;
        self.mac.0[2] = ((mac_low >> 16) & 0xFF) as u8;
        self.mac.0[3] = ((mac_low >> 24) & 0xFF) as u8;
        self.mac.0[4] = (mac_high & 0xFF) as u8;
        self.mac.0[5] = ((mac_high >> 8) & 0xFF) as u8;
    }

    /// Read 8-bit register
    #[inline]
    fn read_reg8(&self, offset: u16) -> u8 {
        unsafe { core::ptr::read_volatile((self.base_addr + offset as usize) as *const u8) }
    }

    /// Write 8-bit register
    #[inline]
    fn write_reg8(&self, offset: u16, value: u8) {
        unsafe {
            core::ptr::write_volatile((self.base_addr + offset as usize) as *mut u8, value);
        }
    }

    /// Read 16-bit register
    #[inline]
    fn read_reg16(&self, offset: u16) -> u16 {
        unsafe { core::ptr::read_volatile((self.base_addr + offset as usize) as *const u16) }
    }

    /// Write 16-bit register
    #[inline]
    fn write_reg16(&self, offset: u16, value: u16) {
        unsafe {
            core::ptr::write_volatile((self.base_addr + offset as usize) as *mut u16, value);
        }
    }

    /// Read 32-bit register
    #[inline]
    fn read_reg32(&self, offset: u16) -> u32 {
        unsafe { core::ptr::read_volatile((self.base_addr + offset as usize) as *const u32) }
    }

    /// Write 32-bit register
    #[inline]
    fn write_reg32(&self, offset: u16, value: u32) {
        unsafe {
            core::ptr::write_volatile((self.base_addr + offset as usize) as *mut u32, value);
        }
    }

    /// Transmit a packet
    fn do_transmit(&mut self, data: &[u8]) -> DevResult {
        if data.len() > TX_BUF_SIZE {
            log::warn!("[RTL8139] TX packet too large: {} > {}", data.len(), TX_BUF_SIZE);
            return Err(DevError::InvalidParam);
        }

        let idx = self.tx_cur;

        // Check if descriptor is available
        let status = self.read_reg32(TSD0 + (idx as u16 * 4));
        if (status & (TSD_OWN | TSD_TOK)) == 0 {
            // Not completed yet
            log::debug!("[RTL8139] TX desc[{}] not available, status={:#x}", idx, status);
            return Err(DevError::Again);
        }

        // Copy data to TX buffer
        let tx_buf = self.tx_buf_vaddr[idx];
        unsafe {
            core::ptr::copy_nonoverlapping(data.as_ptr(), tx_buf as *mut u8, data.len());
        }

        // Pad to minimum Ethernet frame size if needed
        let len = core::cmp::max(data.len(), MIN_ETH_FRAME_SIZE);

        log::debug!("[RTL8139] TX desc[{}]: len={}, TSD offset={:#x}", idx, len, TSD0 + (idx as u16 * 4));

        // Start transmission by writing length to TSD register
        self.write_reg32(TSD0 + (idx as u16 * 4), len as u32);

        // Move to next descriptor
        self.tx_cur = (self.tx_cur + 1) % NUM_TX_DESC;

        log::debug!("[RTL8139] TX triggered, next descriptor: {}", self.tx_cur);
        Ok(())
    }

    /// Receive a packet
    fn do_receive(&mut self) -> DevResult<NetBufPtr> {
        // Check if buffer is empty
        if (self.read_reg8(CR) & CR_BUFE) != 0 {
            return Err(DevError::Again);
        }

        let rx_buf = self.rx_buf_vaddr;
        let cur_rx = self.cur_rx;

        log::debug!("[RTL8139] RX: cur_rx={:#x}, CR={:#x}", cur_rx, self.read_reg8(CR));

        unsafe {
            // Read packet status and length (first 4 bytes)
            let header_ptr = (rx_buf + cur_rx) as *const u32;
            let header = core::ptr::read_volatile(header_ptr);
            let status = (header & 0xFFFF) as u16;
            let total_len = (header >> 16) as usize;

            log::debug!("[RTL8139] RX: status={:#x}, total_len={}", status, total_len);

            // Check receive OK status
            if (status & 0x01) == 0 {
                // Invalid packet, skip it
                log::warn!("[RTL8139] RX: invalid packet, status={:#x}", status);
                self.cur_rx = (cur_rx + total_len + 4 + 3) & !3;
                self.write_reg16(CAPR, (self.cur_rx as u16).wrapping_sub(16));
                return Err(DevError::Again);
            }

            // Validate packet length (excluding 4-byte CRC)
            if total_len < 64 || total_len > 1522 {
                log::warn!("[RTL8139] Invalid packet length: {}", total_len);
                self.cur_rx = (cur_rx + total_len + 4 + 3) & !3;
                self.write_reg16(CAPR, (self.cur_rx as u16).wrapping_sub(16));
                return Err(DevError::BadState);
            }

            // Packet length excluding CRC
            let packet_len = total_len - 4;

            log::debug!("[RTL8139] RX: packet_len={} (total={})", packet_len, total_len);

            // Allocate buffer for packet
            let pages = (packet_len + 4095) / 4096;
            let (pkt_vaddr, _pkt_paddr) = KF::dma_alloc_coherent(pages);
            if pkt_vaddr == 0 {
                log::error!("[RTL8139] Failed to allocate packet buffer");
                return Err(DevError::NoMemory);
            }

            // Copy packet data (skip 4-byte header)
            core::ptr::copy_nonoverlapping(
                (rx_buf + cur_rx + 4) as *const u8,
                pkt_vaddr as *mut u8,
                packet_len,
            );

            // Update cur_rx pointer (4-byte aligned)
            self.cur_rx = (cur_rx + total_len + 4 + 3) & !3;

            // Wrap around if needed
            if self.cur_rx >= RX_BUF_SIZE {
                log::debug!("[RTL8139] RX buffer wrap: {} -> {}", self.cur_rx, self.cur_rx % RX_BUF_SIZE);
                self.cur_rx = self.cur_rx % RX_BUF_SIZE;
            }

            // Update CAPR register (minus 16 to avoid overflow)
            let capr_val = (self.cur_rx as u16).wrapping_sub(16);
            self.write_reg16(CAPR, capr_val);
            log::debug!("[RTL8139] RX: updated CAPR={:#x}, next cur_rx={:#x}", capr_val, self.cur_rx);

            let raw_ptr = NonNull::new(pkt_vaddr as *mut u8).unwrap();
            let buf_ptr = raw_ptr;

            Ok(NetBufPtr::new(raw_ptr, buf_ptr, packet_len))
        }
    }

    pub fn irq_number(&self) -> u8 {
        self.irq
    }
}

impl BaseDriverOps for Rtl8139Driver {
    fn device_name(&self) -> &str {
        "rtl8139"
    }

    fn device_type(&self) -> DeviceType {
        DeviceType::Net
    }
}

impl NetDriverOps for Rtl8139Driver {
    fn mac_address(&self) -> EthernetAddress {
        self.mac
    }

    fn can_transmit(&self) -> bool {
        true
    }

    fn can_receive(&self) -> bool {
        true
    }

    fn rx_queue_size(&self) -> usize {
        1
    }

    fn tx_queue_size(&self) -> usize {
        NUM_TX_DESC
    }

    fn recycle_rx_buffer(&mut self, rx_buf: NetBufPtr) -> DevResult {
        let vaddr = rx_buf.raw_ptr::<u8>() as usize;
        let pages = (rx_buf.packet_len() + 4095) / 4096;
        KF::dma_free_coherent(vaddr, pages);
        Ok(())
    }

    fn recycle_tx_buffers(&mut self) -> DevResult {
        // TX buffers are reused in place
        Ok(())
    }

    fn transmit(&mut self, tx_buf: NetBufPtr) -> DevResult {
        let data = tx_buf.packet();
        let result = self.do_transmit(data);

        // Free the buffer after transmission attempt
        self.recycle_rx_buffer(tx_buf)?;
        result
    }

    fn receive(&mut self) -> DevResult<NetBufPtr> {
        self.do_receive()
    }

    fn alloc_tx_buffer(&mut self, size: usize) -> DevResult<NetBufPtr> {
        if size > TX_BUF_SIZE {
            return Err(DevError::InvalidParam);
        }

        let pages = (size + 4095) / 4096;
        let (vaddr, _paddr) = KF::dma_alloc_coherent(pages);

        if vaddr == 0 {
            return Err(DevError::NoMemory);
        }

        let raw_ptr = NonNull::new(vaddr as *mut u8).unwrap();
        let buf_ptr = raw_ptr;

        Ok(NetBufPtr::new(raw_ptr, buf_ptr, size))
    }
}

impl Drop for Rtl8139Driver {
    fn drop(&mut self) {
        // Disable receiver and transmitter
        self.write_reg8(CR, 0x00);

        // Free receive buffer
        if self.rx_buf_vaddr != 0 {
            let pages = (RX_BUF_SIZE + 4095) / 4096;
            KF::dma_free_coherent(self.rx_buf_vaddr, pages);
        }

        // Free transmit buffers
        let tx_pages = (TX_BUF_SIZE + 4095) / 4096;
        for i in 0..NUM_TX_DESC {
            if self.tx_buf_vaddr[i] != 0 {
                KF::dma_free_coherent(self.tx_buf_vaddr[i], tx_pages);
            }
        }
    }
}
