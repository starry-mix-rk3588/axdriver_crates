//! Wrapper for KernelFunc interface calls
//!
//! This module provides a simplified interface for calling kernel functions
//! through the crate_interface mechanism.

/// Kernel function interface for memory management and system services
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

    /// Get current time in microseconds
    fn get_time_us() -> u64;

    /// Busy wait for specified duration
    fn busy_wait(duration: core::time::Duration);
}

/// Wrapper struct for kernel function calls
pub struct UseKernelFunc;

impl UseKernelFunc {
    /// Convert virtual address to physical address
    #[inline]
    pub fn virt_to_phys(addr: usize) -> usize {
        crate_interface::call_interface!(KernelFunc::virt_to_phys(addr))
    }

    /// Convert physical address to virtual address
    #[inline]
    pub fn phys_to_virt(addr: usize) -> usize {
        crate_interface::call_interface!(KernelFunc::phys_to_virt(addr))
    }

    /// Allocate DMA coherent memory
    ///
    /// # Returns
    /// (virtual_address, physical_address)
    #[inline]
    pub fn dma_alloc_coherent(pages: usize) -> (usize, usize) {
        crate_interface::call_interface!(KernelFunc::dma_alloc_coherent(pages))
    }

    /// Free DMA coherent memory
    #[inline]
    pub fn dma_free_coherent(vaddr: usize, pages: usize) {
        crate_interface::call_interface!(KernelFunc::dma_free_coherent(vaddr, pages))
    }

    /// Get current time in microseconds
    #[inline]
    pub fn get_time_us() -> u64 {
        crate_interface::call_interface!(KernelFunc::get_time_us())
    }

    /// Busy wait for specified duration
    #[inline]
    pub fn busy_wait(duration: core::time::Duration) {
        crate_interface::call_interface!(KernelFunc::busy_wait(duration))
    }
}
