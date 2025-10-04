//! Currently the Async Writing is unstable and may change in the future.
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};

use crate::{EncodingError, ReadableObjectType, WritableObjectType};
/// An asynchronous version of `WritableObjectType`.
pub trait AsyncWritableObjectType: WritableObjectType {
    /// Asynchronously writes the object to a writer.
    ///
    /// Default Implementation writes the objects to a buffer and then writes the buffer to the writer.
    fn write_to_async_writer<W>(
        &self,
        writer: &mut W,
    ) -> impl Future<Output = Result<(), EncodingError>> + Send
    where
        Self: Sync,
        W: AsyncWrite + Unpin + Send,
    {
        async move {
            let result = self.write_to_bytes()?;
            writer
                .write_all(&result)
                .await
                .map_err(EncodingError::IOError)?;
            Ok(())
        }
    }
}

pub trait AsyncReadableObjectType: ReadableObjectType {
    /// Asynchronously reads the object from a reader.
    fn read_from_async_reader<R>(
        reader: &mut R,
    ) -> impl Future<Output = Result<Self, EncodingError>> + Send
    where
        Self: Sync + Sized,
        R: AsyncRead + Unpin + Send;
}
