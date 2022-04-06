# msfs2020checksum - checksum tool for MSFS2020 package files

## Introduction
Microsoft Flight Simulator 2020 (MSFS2020) has a ton of data packages.
However, I don't seem to find that Microsoft provides a user-visible data integrity verification mechanism.
So I wrote this little tool in Rust: it can automatically locate the location of MSFS2020 packages, and compute its hash value for each file.
After the calculation is complete, you can compare it to hashes computed by other users with the same MSFS version to determine if there is any file corruption.

This tool uses the 128-bit [xxHash](https://github.com/DoumanAsh/xxhash-rust) algorithm (aka XXH128) and automatically uses all CPU cores for parallel computing.. So it generates hashes very fast, and the performance bottleneck is almost solely determined by the read speed of your hard drive.

## System requirements
The following are the requirements for running the msfs2020checksum tool (not for MSFS2020 itself):
* Windows 10 21H1 or later 64-bit operating system (Not available for Xbox or Linux).
* MSFS2020 has been properly installed and has been run at least once.
* CPUs that support AVX2 instructions.
* At least 8GB RAM.
* If the MSFS2020's package files are placed on the SSD, there will be a big performance improvement.

## Usage
1. Open the Command Prompt or PowerShell in the directory of `msfs2020checksum.exe` file and then run this command:
(Replace `D:\MyMSFS.xxhash` with the path you want to save to)  
(Warning: If the output file already exists, it will be overwritten.)
    ```
    msfs2020checksum.exe -o D:\MyMSFS.xxhash
    ```
2. Use a text comparison tool you are familiar with to compare. In this example, I'm using the free [VSCode](https://code.visualstudio.com/) to compare with the target file (For example, `E:\MSFSGoodfiles.xxhash`).
    ```
    code --diff D:\MyMSFS.xxhash E:\MSFSGoodfiles.xxhash
    ```
3. The first column of the output is the hash value, the second column is the file size, and the third column is the file path.
4. For the comparison results, you can refer to [here](compare_doc/README.md).

## Technical notes
* I only have the MS Store version of MSFS 2020 on which this tool is tested and available.
This tool theoretically supports the Steam version, but I didn't test it.
* There should be no symbolic links or hard links in the package directory.
If there are symlinks or hard links, there may be some unexpected results.
* Use this command `msfs2020checksum.exe -h` to see the usage of more arguments.

## License
This tool is primarily distributed under the terms of the Boost Software License (Version 1.0).  
See [LICENSE](LICENSE) for details.

### Contribution
1. Any contribution intentionally submitted for inclusion in msfs2020checksum by
  you, as defined in the Boost Software License, shall be licensed as above,
  without any additional terms or conditions.
2. Pull requests are always welcome.
