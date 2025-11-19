use alloc::boxed::Box;
use alloc::collections::VecDeque;
use alloc::vec::Vec;
use axdriver_base::{BaseDriverOps, DevError, DevResult, DeviceType};
use core::ptr::NonNull;
use log::*;
use realtek_drivers::rtl8169::Rtl8169;

use crate::NetDriverOps;
pub use realtek_drivers::{KernelFunc, UseKernelFunc};

const RX_QUEUE_SIZE: usize = 64;

pub struct RealtekNic {
    inner: Rtl8169,
    rx_buffer_queue: VecDeque<crate::NetBufPtr>,
}

unsafe impl Sync for RealtekNic {}
unsafe impl Send for RealtekNic {}

impl RealtekNic {
    pub fn init(mmio_virt: usize) -> DevResult<Self> {
        info!("RealtekNic init @ {:#x}", mmio_virt);
        let rx_buffer_queue = VecDeque::with_capacity(RX_QUEUE_SIZE);

        // let inner = Rtl8125::new(mmio_phys.into());

        // let local_ip = [192, 168, 22, 102];

        // crate::netstack::test_ping(inner, local_ip);

        let mut inner = Rtl8169::new(mmio_virt.into(), 0x8125);
        inner.eth_probe();
        inner.eth_start();

        let dev = Self {
            inner,
            rx_buffer_queue,
        };
        Ok(dev)
    }
}

impl BaseDriverOps for RealtekNic {
    fn device_name(&self) -> &str {
        "cdns,phytium-gem-1.0"
    }

    fn device_type(&self) -> DeviceType {
        DeviceType::Net
    }
}

impl NetDriverOps for RealtekNic {
    fn mac_address(&self) -> crate::EthernetAddress {
        let mac_addr = self.inner.mac_address();
        crate::EthernetAddress(mac_addr)
    }

    fn can_transmit(&self) -> bool {
        true
    }

    fn can_receive(&self) -> bool {
        !self.rx_buffer_queue.is_empty()
    }

    fn rx_queue_size(&self) -> usize {
        RX_QUEUE_SIZE
    }

    fn tx_queue_size(&self) -> usize {
        RX_QUEUE_SIZE
    }

    fn recycle_rx_buffer(&mut self, rx_buf: crate::NetBufPtr) -> DevResult {
        unsafe {
            drop(Box::from_raw(rx_buf.raw_ptr::<Vec<u8>>()));
        }
        drop(rx_buf);
        Ok(())
    }

    fn recycle_tx_buffers(&mut self) -> DevResult {
        Ok(())
    }

    fn transmit(&mut self, tx_buf: crate::NetBufPtr) -> DevResult {
        self.inner.eth_send(tx_buf.packet(), tx_buf.packet_len());
        Ok(())
    }

    fn receive(&mut self) -> DevResult<crate::NetBufPtr> {
        if !self.rx_buffer_queue.is_empty() {
            // RX buffer queue has received packets
            Ok(self.rx_buffer_queue.pop_front().unwrap())
        } else {
            // Try to receive new packet from hardware
            let mut xbuf = [0u8; 1536];
            let recv_len = self.inner.eth_recv(&mut xbuf) as usize;
            if recv_len > 0 {
                debug!("received packet length {}", recv_len);
                // Create a Vec with actual received length
                let packet = xbuf[..recv_len].to_vec();
                let mut buf = Box::new(packet);
                let buf_ptr = buf.as_mut_ptr() as *mut u8;
                let buf_len = buf.len();

                let rx_buf = crate::NetBufPtr::new(
                    NonNull::new(Box::into_raw(buf) as *mut u8).unwrap(),
                    NonNull::new(buf_ptr).unwrap(),
                    buf_len,
                );

                self.rx_buffer_queue.push_back(rx_buf);
                Ok(self.rx_buffer_queue.pop_front().unwrap())
            } else {
                Err(DevError::Again)
            }
        }
    }

    fn alloc_tx_buffer(&mut self, size: usize) -> DevResult<crate::NetBufPtr> {
        let mut tx_buf = Box::new(alloc::vec![0; size]);
        let tx_buf_ptr = tx_buf.as_mut_ptr();

        Ok(crate::NetBufPtr::new(
            NonNull::new(Box::into_raw(tx_buf) as *mut u8).unwrap(),
            NonNull::new(tx_buf_ptr).unwrap(),
            size,
        ))
    }
}
