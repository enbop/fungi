trait Device {}

trait DeviceManager {
    type DeviceImpl: Device;
    type Error: std::error::Error;

    async fn can_access(
        &self,
        target: impl AsRef<str>,
        from: impl AsRef<str>,
    ) -> Result<bool, Self::Error>;

    async fn add_device(
        &self,
        id: impl AsRef<str>,
        device: Self::DeviceImpl,
    ) -> Result<(), Self::Error>;
    async fn get_device(&self, id: impl AsRef<str>) -> Result<Self::DeviceImpl, Self::Error>;
    async fn remove_device(&self, id: impl AsRef<str>) -> Result<(), Self::Error>;
}
