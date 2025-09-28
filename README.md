# Tux IO Encoding

An encoding format for Objects stored on disk.

## Overview
The Object layout is as follows
```
| File Header 32 Bytes |
| Metadata (Key Value Pairs) (System Controlled) |
| Tags (User Controlled) |
| Content (User Controlled) |
```
### Content Alignment and Padding
When a new file is created. After the file metadata and tags are written we will add padding up to ensure that the content atleast starts at the 256 byte mark.

After that. They will be aligned to 32 byte boundaries. The purpose of this is to ensure that small changes to the file tags or metadata do not require rewriting the entire file.


### File Head
The File Header is 32 bytes long and contains metadata about the file. It is used to identify the file format and provide information about the content.
After the first 4 bytes. The content and ordering could change in the future.

| Name              | Size | Note |
| -------------     | ---- | ------ |
| Magic Value       | 3 bytes | A byte array of b'TUX' or [0x54, 0x55, 0x58] |
| Version           | 1 byte  | Currently 0 |
| Tags Start        | 2 bytes  | Starting Byte for the tags. This includes the size of the ObjectHeader if set to 0 no tags |
| Compression Type  | 1 byte   | See Compression Type Below |
| Content Start     | 4 bytes  | Starting Byte for the content. This includes the size of the ObjectHeader and the tags. |
| Content Length    | 8 bytes  |  |
| Bit Flags         | 1 byte   | Bit flags for additional metadata(Reserved should be 0 until defined layer) |
| PlaceHolder/Reserved | 12 bytes| I wanted this to have extra room just in case. Also makes this object an even 32 bytes|

### File Metadata
File Meta is stored in the same structure as Tags.

#### Difference from Tags
- Tags are user controlled and can be added or removed at any time.
- Metadata is always read. Whenever the file is opened the Metadata is also read. Keys and Values are constrained to the rules for Headers. Keys are always lowercase for example. Metadata is mostly immutable. Most of the metadata will be computed on file creation
- When used inside the S3 service metadata is completely immutable however, if this encoding is used outside of S3 it may be modified as needed.

#### Suggestions for Metadata
Metadata should be as small as possible and only store data that is deemed necessary for the system to function. The overall goal of metadata is also to be searchable. So you can say get me the value of the `content_type`  It will not return all

### Data Types
| data type     | Type Key |
| ------------- | ---------  |
| byte          | 0     |
| u16           | 1     |
| u32           | 2     |
| u64           | 3     |
| i8            | 4     |
| i16           | 5     |
| i32           | 6     |
| i64           | 7     |
| f32           | 8     |
| f64           | 9     |
| boolean       | 10    |
| byte array    | 11    |
| string        | 12    |
| date          | 13    |
| time          | 14    |
| timezone      | 15    |
| datetime      | 16    |
| uuid          | 17    |

#### String
All strings are UTF-8 encoded and limited to 65535 bytes. The string is prefixed with a u16 length field.
##### Fixed Size Strings
Any String that is marked as fixed size string will be padded with null bytes to the specified length. The padding is done on the right side of the string.
#### Byte Array
Are like strings but can contain any byte value. They are also prefixed with a u16 length field.
#### Numbers
All numbers are stored in little-endian format.
#### Date And Time
##### Date
`{year:u16}{month:u8}{day:u8}`
##### Time
`{seconds_from_midnight:u32}{nanoseconds:u32}`
##### Timezone
`{seconds_from_utc:i32}`
##### Full DateTime
`{date:Date}{time:Time}{timezone:Timezone}`

#### Key Value Pairs (Maps, Tags Whatever)
Key Value Pairs are a data structure that allows for storing key-value pairs.
The first 2 bytes are the number of pairs in the map. So the limit is 65535 pairs.
The key is a string using the same format as the string data type.
Following will be a single byte that indicates the data type of the value.
The value is then encoded using the data type format.
`{number of pairs:u16}{key: string}{type_id: u8}{value: data type}`

## Compression
Compression is used on the object content to reduce file size. The compression type is specified in the file header and can be one of the following:
| Name              | Key | Other |
| -------------     | ---- | ------ |
| None              | 0   |  |