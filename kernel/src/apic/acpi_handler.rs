use acpi::{AcpiHandler, PhysicalMapping};
use core::ptr::NonNull;

#[derive(Clone)]
pub struct IdentityAcpiHandler;

impl AcpiHandler for IdentityAcpiHandler {
    unsafe fn map_physical_region<T>(
        &self,
        physical_address: usize,
        size: usize,
    ) -> PhysicalMapping<Self, T> {
        // We must map the physical address to virtual memory. 
        // We will use our memory module to strictly identity-map the required pages.
        let start_page = physical_address & !0xFFF;
        let end_page = (physical_address + size + 0xFFF) & !0xFFF;
        
        crate::memory::map_identity_region(start_page as u64, end_page as u64);
        
        unsafe {
            PhysicalMapping::new(
                physical_address,
                NonNull::new(physical_address as *mut T).unwrap(),
                size,
                size,
                self.clone(),
            )
        }
    }

    fn unmap_physical_region<T>(_region: &PhysicalMapping<Self, T>) {
        // For simplicity, we just leave ACPI physical frames mapped.
    }
}
